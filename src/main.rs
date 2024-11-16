#![no_std]
#![no_main]
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unreachable_code)]

use panic_semihosting as _; // panic handler

mod pitched_channel;
mod types;

use rtic_monotonics::rp2040::prelude::*;

rp2040_timer_monotonic!(Mono);

#[rtic::app(device = rp_pico::hal::pac, dispatchers = [SW0_IRQ, SW1_IRQ, SW2_IRQ])]
mod midi_master {

    use ::nb::Error;
    use embedded_hal::can::nb;
    use embedded_hal::digital::v2::{InputPin, OutputPin};
    use fugit::Duration;
    use rp_pico::hal;

    use fugit::RateExtU32;

    use hal::gpio::bank0::Gpio25;
    use hal::gpio::Pin;
    use rp_pico::hal::gpio::FunctionSio;
    use rp_pico::hal::gpio::PullDown;
    use rp_pico::hal::gpio::SioOutput;

    use core::fmt::Write;
    use rtic_monotonics::rp2040::prelude::*;

    // use usb_device;
    // use usb_device::class_prelude::UsbBusAllocator;
    // use usb_device::prelude::UsbDeviceBuilder;
    // use usb_device::prelude::UsbVidPid;

    use midly::{live::LiveEvent, MidiMessage};

    use rtic_sync::{channel::*, make_channel};

    use hal::gpio;
    use hal::pwm;

    use crate::pitched_channel::{
        get_pitched_channel, ChannelQuartet, FourVoiceChannel, PitchedChannel, SingleVoiceChannel,
    };
    use crate::Mono;

    // use crate::pitched_channel;
    // use crate::pitched_channel::GpvChannel;
    use crate::types::*;

    const MESSAGE_CAPACITY: usize = 16;
    const PITCHED_CHANELL: midly::num::u4 = midly::num::u4::new(1);
    const DRUM_CHANELL: midly::num::u4 = midly::num::u4::new(5);
    type MessageSender<T> = Sender<'static, T, MESSAGE_CAPACITY>;
    type MessageReceiver<T> = Receiver<'static, T, MESSAGE_CAPACITY>;
    type UartType = hal::uart::UartPeripheral<
        hal::uart::Enabled,
        hal::pac::UART0,
        (
            Pin<gpio::bank0::Gpio0, gpio::FunctionUart, gpio::PullDown>,
            Pin<gpio::bank0::Gpio1, gpio::FunctionUart, gpio::PullDown>,
        ),
    >;

    fn blink(
        led: &mut Pin<Gpio25, FunctionSio<SioOutput>, PullDown>,
        duration: Duration<u64, 1, 1_000_000>,
        hold_down: bool,
    ) {
        led.set_high().unwrap();
        let quick = Mono::now() + duration;
        while Mono::now() < quick {}
        led.set_low().unwrap();
        if hold_down {
            let quick = Mono::now() + duration / 2;
            while Mono::now() < quick {}
        }
    }

    #[local]
    struct Local {
        //     usb_bus: UsbBusAllocator<hal::usb::UsbBus>,
        uart_sender: MessageSender<heapless::String<256>>,
        watchdog: hal::Watchdog,
        uart: UartType,
        midi_sender: MessageSender<LiveEvent<'static>>,
        clock_high: bool,
        drums: Drums,
        bus: Bus,
        midi_instance: (Option<FourVoiceChannel>, Option<ChannelQuartet>),
        divide_clock: bool,
    }

    #[shared]
    struct Shared {
        led: gpio::Pin<Gpio25, gpio::FunctionSioOutput, gpio::PullDown>,
    }

    #[init()]
    fn init(c: init::Context) -> (Shared, Local) {
        unsafe {
            hal::sio::spinlock_reset();
        }

        let mut resets = c.device.RESETS;
        Mono::start(c.device.TIMER, &resets);

        let watchdog_timeout = c.device.WATCHDOG.reason().read().timer().bit_is_set();

        let mut watchdog = hal::Watchdog::new(c.device.WATCHDOG);
        let clocks = hal::clocks::init_clocks_and_plls(
            rp_pico::XOSC_CRYSTAL_FREQ,
            c.device.XOSC,
            c.device.CLOCKS,
            c.device.PLL_SYS,
            c.device.PLL_USB,
            &mut resets,
            &mut watchdog,
        )
        .ok()
        .unwrap();

        let sio = hal::Sio::new(c.device.SIO);
        let pins = hal::gpio::Pins::new(
            c.device.IO_BANK0,
            c.device.PADS_BANK0,
            sio.gpio_bank0,
            &mut resets,
        );

        let mut led = pins.gpio25.into_push_pull_output();

        led.set_high().unwrap();
        if watchdog_timeout {
            led.set_low().unwrap();
            let mut blinks = 5;
            while blinks > 0 {
                blink(&mut led, 100.millis(), true);
                blinks -= 1;
            }
        }

        // ConFig

        let divide_clock = pins.gpio2.into_pull_up_input().is_high().unwrap();

        // Drummmms

        let mut drums = Drums {
            open_hh: pins.gpio9.into_push_pull_output(),
            clap: pins.gpio10.into_push_pull_output(),
            snare: pins.gpio11.into_push_pull_output(),
            kick: pins.gpio12.into_push_pull_output(),
            fx: pins.gpio20.into_push_pull_output(),
            accent: pins.gpio21.into_push_pull_output(),
            closed_hh: pins.gpio22.into_push_pull_output(),
        };

        drums.reset();

        let mut bus = Bus {
            start: pins.gpio13.into_push_pull_output(),
            stop: pins.gpio18.into_push_pull_output(),
            clock: pins.gpio19.into_push_pull_output(),
        };

        bus.reset();

        let pwm_slices = hal::pwm::Slices::new(c.device.PWM, &mut resets);

        let midi_conf: MidiConfig = match pins.gpio3.into_pull_up_input().is_high().unwrap() {
            true => MidiConfig::QuadPoly,
            false => MidiConfig::QuadMono,
        };

        let midi_instance = match midi_conf {
            MidiConfig::QuadPoly => (
                Some(FourVoiceChannel::new(
                    pwm_slices.pwm7,
                    pwm_slices.pwm0,
                    (
                        pins.gpio14.into_function::<gpio::FunctionPwm>(),
                        pins.gpio15.into_function::<gpio::FunctionPwm>(),
                    ),
                    (
                        pins.gpio17.into_function::<gpio::FunctionPwm>(),
                        pins.gpio16.into_function::<gpio::FunctionPwm>(),
                    ),
                    pins.gpio5.into_push_pull_output(),
                    pins.gpio6.into_push_pull_output(),
                    pins.gpio7.into_push_pull_output(),
                    pins.gpio8.into_push_pull_output(),
                )),
                None,
            ),
            MidiConfig::QuadMono => (
                None,
                Some(ChannelQuartet::new(
                    [1, 2, 3, 4],
                    pwm_slices.pwm7,
                    pwm_slices.pwm0,
                    (
                        pins.gpio14.into_function::<gpio::FunctionPwm>(),
                        pins.gpio15.into_function::<gpio::FunctionPwm>(),
                    ),
                    (
                        pins.gpio17.into_function::<gpio::FunctionPwm>(),
                        pins.gpio16.into_function::<gpio::FunctionPwm>(),
                    ),
                    pins.gpio5.into_push_pull_output(),
                    pins.gpio6.into_push_pull_output(),
                    pins.gpio7.into_push_pull_output(),
                    pins.gpio8.into_push_pull_output(),
                )),
            ),
            _ => (None, None),
        };

        // let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
        //     c.device.USBCTRL_REGS,
        //     c.device.USBCTRL_DPRAM,
        //     clocks.usb_clock,
        //     true,
        //     &mut resets,
        // ));

        // Set up UART on GP0 and GP1 (Pico pins 1 and 2)
        let uart_pins = (
            pins.gpio0.into_function::<gpio::FunctionUart>(),
            pins.gpio1.into_function::<gpio::FunctionUart>(),
        );

        // Need to perform clock init before using UART or it will freeze.
        let mut uart = hal::uart::UartPeripheral::new(c.device.UART0, uart_pins, &mut resets)
            .enable(
                hal::uart::UartConfig::new(
                    31250.Hz(),
                    hal::uart::DataBits::Eight,
                    None,
                    hal::uart::StopBits::One,
                ),
                hal::Clock::freq(&clocks.peripheral_clock),
            )
            .unwrap();
        uart.enable_rx_interrupt();

        let (uart_sender, _uart_receiver) = make_channel!(heapless::String<256>, MESSAGE_CAPACITY);
        let (midi_sender, midi_receiver) = make_channel!(LiveEvent, MESSAGE_CAPACITY);

        watchdog.start(fugit::ExtU32::micros(10_000));
        watchdog_feeder::spawn().ok();
        // usb_handler::spawn(uart_receiver).ok();
        midi_handler::spawn(midi_receiver).ok();
        // test_suite::spawn(midi_sender.clone() /*, uart_sender.clone()*/).ok();

        return (
            Shared { led },
            Local {
                //        usb_bus,
                uart_sender: uart_sender.clone(),
                uart,
                watchdog,
                midi_sender,
                clock_high: false,
                drums,
                midi_instance,
                bus,
                divide_clock,
            },
        );
    }

    #[task(priority=0, shared = [])]
    async fn test_suite(
        _c: test_suite::Context,
        mut midi_sender: MessageSender<LiveEvent<'_>>,
        // mut uart_sender: MessageSender<heapless::String<256>>,
    ) {
        let mut test_step: u8 = 36;
        let mut on: bool = true;
        let mut octave: u8 = 0;
        let drum = 36;
        // uart_sender.try_send(heapless::String::from("Testing")).ok();
        loop {
            if test_step > 42 {
                test_step = 36;
                octave = (octave + 1) % 6;
                on = !on;
            }
            midi_sender
                .try_send(LiveEvent::Realtime(
                    midly::live::SystemRealtime::TimingClock,
                ))
                .ok();
            if on {
                midi_sender
                    .try_send(LiveEvent::Midi {
                        channel: PITCHED_CHANELL,
                        message: MidiMessage::NoteOn {
                            key: midly::num::u7::from(test_step + 1),
                            vel: midly::num::u7::from(0),
                        },
                    })
                    .ok();
            } else {
                midi_sender
                    .try_send(LiveEvent::Midi {
                        channel: PITCHED_CHANELL,
                        message: MidiMessage::NoteOff {
                            key: midly::num::u7::from(test_step),
                            vel: midly::num::u7::from(0),
                        },
                    })
                    .ok();
            }
            midi_sender
                .try_send(LiveEvent::Midi {
                    channel: DRUM_CHANELL,
                    message: MidiMessage::NoteOn {
                        key: midly::num::u7::from(drum + octave),
                        vel: midly::num::u7::from(0),
                    },
                })
                .ok();
            // } else {
            midi_sender
                .try_send(LiveEvent::Midi {
                    channel: DRUM_CHANELL,
                    message: MidiMessage::NoteOff {
                        key: midly::num::u7::from(drum + octave - 1),
                        vel: midly::num::u7::from(0),
                    },
                })
                .ok();
            // }

            Mono::delay(100_000.micros()).await;
            test_step += 1;
        }
    }

    #[task(local = [drums, bus, midi_instance, divide_clock], shared=[])]
    async fn midi_handler(c: midi_handler::Context, mut receiver: MessageReceiver<LiveEvent<'_>>) {
        let mut clock_pulse_count: u16 = 0;
        let ppq = 24;
        let subdiv = match c.local.divide_clock {
            true => ppq - 1,
            false => 2,
        }; // per quarter
        loop {
            match receiver.recv().await {
                Ok(LiveEvent::Midi { channel, message }) => match channel {
                    DRUM_CHANELL => match message {
                        MidiMessage::NoteOn { key, vel: _ } => c.local.drums.set(key, true),
                        MidiMessage::NoteOff { key, vel: _ } => c.local.drums.set(key, false),
                        _ => {}
                    },
                    channel => match get_pitched_channel(c.local.midi_instance) {
                        Some(thing) => match message {
                            MidiMessage::NoteOn { key, vel: _ } => {
                                thing.note_on(u8::from(key), channel.into())
                            }
                            MidiMessage::NoteOff { key, vel: _ } => {
                                thing.note_off(u8::from(key), channel.into())
                            }
                            _ => {}
                        },
                        None => {}
                    },
                },
                Ok(LiveEvent::Realtime(event_type)) => match event_type {
                    midly::live::SystemRealtime::TimingClock => {
                        c.local
                            .bus
                            .set(BusSignals::CLOCK, (clock_pulse_count % (subdiv)) == 0);
                        if (clock_pulse_count % (subdiv)) == 0 {
                            c.local.bus.set(BusSignals::STOP, false);
                            c.local.bus.set(BusSignals::START, false);
                        }
                        clock_pulse_count = (clock_pulse_count + 1) % ppq;
                    }
                    midly::live::SystemRealtime::Stop => {
                        get_pitched_channel(c.local.midi_instance)
                            .unwrap()
                            .all_notes_off();
                        c.local.bus.set(BusSignals::CLOCK, false);
                        c.local.bus.set(BusSignals::STOP, true);
                    }
                    midly::live::SystemRealtime::Start => c.local.bus.set(BusSignals::START, true),
                    _ => {}
                },
                Ok(LiveEvent::Common(_)) => {}
                Err(_) => {} // Errors are for the weak
            }
        }
    }

    #[task(local = [clock_high, uart_sender, midi_sender, uart], shared=[led], binds=UART0_IRQ)]
    fn uart(mut c: uart::Context) {
        let mut bob = [0u8; 32];
        if !c.local.uart.uart_is_readable() {
            let _ = c.local.uart_sender.try_send(heapless::String::from(
                heapless::String::try_from("Shit aint readable").unwrap(),
            ));
            return;
        }
        match c.local.uart.read_raw(&mut bob) {
            Ok(bytes) => {
                if bytes > 0 {
                    c.local.uart.write_raw(&bob).ok();
                }
                let mut i = 0;
                while i < bytes {
                    match LiveEvent::parse(&bob[i..]) {
                        Ok(LiveEvent::Realtime(message)) => {
                            // Ignoring Clocks and such for now
                            match message {
                                midly::live::SystemRealtime::TimingClock => {}
                                _ => {}
                            }
                            c.local
                                .midi_sender
                                .try_send(LiveEvent::Realtime(message))
                                .ok();
                            i += 1
                        }
                        Ok(LiveEvent::Common(message)) => {
                            //Ignoring comons for now
                            let mut text: heapless::String<256> = heapless::String::new();
                            write!(&mut text, "{:?}\n", message).ok();
                            c.local.uart_sender.try_send(text).ok();
                            i += 3;
                        }
                        Ok(LiveEvent::Midi { channel, message }) => {
                            c.shared.led.lock(|l| l.set_high().unwrap());
                            let mut text: heapless::String<256> = heapless::String::new();
                            write!(&mut text, "C:{} {:?}\n", u8::from(channel) + 1, message).ok();
                            c.local.uart_sender.try_send(text).ok();
                            c.local
                                .midi_sender
                                .try_send(LiveEvent::Midi {
                                    channel: midly::num::u4::from(u8::from(channel) + 1),
                                    message,
                                })
                                .ok();
                            i += 3;
                        }
                        Err(e) => {
                            let mut text: heapless::String<256> = heapless::String::new();
                            write!(&mut text, " {:?}", e).ok();
                            c.local.uart_sender.try_send(text).ok();
                            i += 1;
                        }
                    }
                }
            }
            Err(Error::WouldBlock) => {}
            Err(Error::Other(_)) => {}
        };
    }

    #[task(priority = 1, shared = [], local = [watchdog])]
    async fn watchdog_feeder(c: watchdog_feeder::Context) {
        loop {
            c.local.watchdog.feed();
            Mono::delay(1000.micros()).await;
        }
    }
}
