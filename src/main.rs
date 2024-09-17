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

    use embedded_hal::digital::v2::OutputPin;
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

    use crate::pitched_channel::FourVoiceChannel;
    use crate::pitched_channel::GpvChannel;
    use crate::Mono;

    // use crate::pitched_channel;
    // use crate::pitched_channel::GpvChannel;
    use crate::types::*;

    const MESSAGE_CAPACITY: usize = 16;
    const PITCHED_CHANELL: midly::num::u4 = midly::num::u4::new(1);
    const DRUM_CHANELL: midly::num::u4 = midly::num::u4::new(2);
    type MessageSender<T> = Sender<'static, T, MESSAGE_CAPACITY>;
    type MessageReceiver<T> = Receiver<'static, T, MESSAGE_CAPACITY>;
    // type UartType = hal::uart::UartPeripheral<
    //     hal::uart::Enabled,
    //     hal::pac::UART0,
    //     (
    //         Pin<gpio::bank0::Gpio0, gpio::FunctionUart, gpio::PullDown>,
    //         Pin<gpio::bank0::Gpio1, gpio::FunctionUart, gpio::PullDown>,
    //     ),
    // >;

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
        //    uart_sender: MessageSender<heapless::String<256>>,
        watchdog: hal::Watchdog,
        //    uart: UartType,
        midi_sender: MessageSender<LiveEvent<'static>>,
        clock_high: bool,
        drums: Drums,
        bus: Bus,
        pitched_channel: FourVoiceChannel,
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
        } else {
            blink(&mut led, 500.millis(), false);
        }

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

        /*

                    pairs: (
                        CvPair::new(
                            slices.pwm7,
                            pins.gpio14.into_function::<gpio::FunctionPwm>(),
                            pins.gpio15.into_function::<gpio::FunctionPwm>(),
                        ),
                        CvPair::new(
                            slices.pwm0,
                            pins.gpio16.into_function::<gpio::FunctionPwm>(),
                            pins.gpio17.into_function::<gpio::FunctionPwm>(),
                        ),
                    ),
                    notes: [None, None, None, None],
                    gates: [
                        PwmGate::GateA(pins.gpio5.into_push_pull_output()),
                        PwmGate::GateB(pins.gpio6.into_push_pull_output()),
                        PwmGate::GateC(pins.gpio7.into_push_pull_output()),
                        PwmGate::GateD(pins.gpio8.into_push_pull_output()),
                    ],


        */

        let pitched_channel = FourVoiceChannel::new(
            pwm_slices.pwm7,
            pwm_slices.pwm0,
            pins.gpio14.into_function::<gpio::FunctionPwm>(),
            pins.gpio15.into_function::<gpio::FunctionPwm>(),
            pins.gpio17.into_function::<gpio::FunctionPwm>(),
            pins.gpio16.into_function::<gpio::FunctionPwm>(),
            pins.gpio5.into_push_pull_output(),
            pins.gpio6.into_push_pull_output(),
            pins.gpio7.into_push_pull_output(),
            pins.gpio8.into_push_pull_output(),
        );

        // let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
        //     c.device.USBCTRL_REGS,
        //     c.device.USBCTRL_DPRAM,
        //     clocks.usb_clock,
        //     true,
        //     &mut resets,
        // ));

        // Set up UART on GP0 and GP1 (Pico pins 1 and 2)
        // let uart_pins = (
        //     pins.gpio0.into_function::<gpio::FunctionUart>(),
        //     pins.gpio1.into_function::<gpio::FunctionUart>(),
        // );

        // // Need to perform clock init before using UART or it will freeze.
        // let mut uart = hal::uart::UartPeripheral::new(c.device.UART0, uart_pins, &mut resets)
        //     .enable(
        //         hal::uart::UartConfig::new(
        //             31250.Hz(),
        //             hal::uart::DataBits::Eight,
        //             None,
        //             hal::uart::StopBits::One,
        //         ),
        //         hal::Clock::freq(&clocks.peripheral_clock),
        //     )
        //     .unwrap();
        // uart.enable_rx_interrupt();

        // let gate = pins.gpio5.into_push_pull_output();
        // let a_pin = pins.gpio14.into_function::<gpio::FunctionPwm>();
        // let b_pin = ;

        // let pitched_channel = GpvChannel::new(
        //     1,
        //     PwmGate::GateA(pins.gpio5.into_push_pull_output()),
        //     pwm_slices.pwm7,
        //     (
        //         pins.gpio14.into_function::<gpio::FunctionPwm>(),
        //         pins.gpio15.into_function::<gpio::FunctionPwm>(),
        //     ),
        // );

        // let (uart_sender, uart_receiver) = make_channel!(heapless::String<256>, MESSAGE_CAPACITY);
        let (midi_sender, midi_receiver) = make_channel!(LiveEvent, MESSAGE_CAPACITY);

        watchdog.start(fugit::ExtU32::micros(10_000));
        watchdog_feeder::spawn().ok();
        // usb_handler::spawn(uart_receiver).ok();
        midi_handler::spawn(midi_receiver).ok();
        test_suite::spawn(midi_sender.clone() /*, uart_sender.clone()*/).ok();

        return (
            Shared { led },
            Local {
                //        usb_bus,
                //      uart_sender: uart_sender.clone(),
                //      uart,
                watchdog,
                midi_sender,
                clock_high: false,
                drums,
                pitched_channel,
                bus,
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
                let note = octave * 12 + 12;
                if on {
                    midi_sender
                        .try_send(LiveEvent::Midi {
                            channel: PITCHED_CHANELL,
                            message: MidiMessage::NoteOn {
                                key: midly::num::u7::from(note),
                                vel: midly::num::u7::from(0),
                            },
                        })
                        .ok();
                } else {
                    midi_sender
                        .try_send(LiveEvent::Midi {
                            channel: PITCHED_CHANELL,
                            message: MidiMessage::NoteOff {
                                key: midly::num::u7::from(note),
                                vel: midly::num::u7::from(0),
                            },
                        })
                        .ok();
                }
                on = !on;
            }
            midi_sender
                .try_send(LiveEvent::Realtime(
                    midly::live::SystemRealtime::TimingClock,
                ))
                .ok();
            // if on {
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

    #[task(local = [drums, bus, pitched_channel], shared=[])]
    async fn midi_handler(c: midi_handler::Context, mut receiver: MessageReceiver<LiveEvent<'_>>) {
        let mut clock_pulse_count: u16 = 0;
        let ppq = 24;
        let subdiv = 4; // per quarter
        loop {
            match receiver.recv().await {
                Ok(LiveEvent::Midi { channel, message }) => match channel {
                    PITCHED_CHANELL => match message {
                        MidiMessage::NoteOn { key, vel: _ } => {
                            c.local.pitched_channel.note_on(u8::from(key))
                        }
                        MidiMessage::NoteOff { key, vel: _ } => {
                            c.local.pitched_channel.note_off(u8::from(key))
                        }
                        // MidiMessage::Aftertouch { key, vel } => c
                        //     .local
                        //     .pitched_channel
                        //     .aftertouch(u8::from(key), u8::from(vel)),
                        _ => {}
                    },
                    DRUM_CHANELL => match message {
                        MidiMessage::NoteOn { key, vel: _ } => c.local.drums.set(key, true),
                        MidiMessage::NoteOff { key, vel: _ } => c.local.drums.set(key, false),
                        _ => {}
                    },
                    _ => {}
                },
                Ok(LiveEvent::Realtime(event_type)) => match event_type {
                    midly::live::SystemRealtime::TimingClock => {
                        c.local
                            .bus
                            .set(BusSignals::CLOCK, (clock_pulse_count % (ppq / subdiv)) == 0);
                        c.local.bus.set(BusSignals::STOP, false);
                        c.local.bus.set(BusSignals::START, false);
                        clock_pulse_count = (clock_pulse_count + 1) % ppq;
                    }
                    midly::live::SystemRealtime::Stop => c.local.bus.set(BusSignals::STOP, true),
                    midly::live::SystemRealtime::Start => c.local.bus.set(BusSignals::START, true),
                    _ => {}
                },
                Ok(LiveEvent::Common(_)) => {}
                Err(_) => {} // Errors are for then weak
            }
        }
    }

    // #[task(local = [clock_high, uart_sender, midi_sender, uart], shared=[led], binds=UART0_IRQ)]
    // fn uart(mut c: uart::Context) {
    //     let mut bob = [0u8; 32];
    //     if !c.local.uart.uart_is_readable() {
    //         let _ = c
    //             .local
    //             .uart_sender
    //             .try_send(heapless::String::from("Shit aint readable"));
    //         return;
    //     }
    //     match c.local.uart.read_raw(&mut bob) {
    //         Ok(bytes) => {
    //             if bytes > 0 {
    //                 c.local.uart.write_raw(&bob).ok();
    //             }
    //             let mut i = 0;
    //             while i < bytes {
    //                 match LiveEvent::parse(&bob[i..]) {
    //                     Ok(LiveEvent::Realtime(message)) => {
    //                         // Ignoring Clocks and such for now
    //                         match message {
    //                             midly::live::SystemRealtime::TimingClock => {}
    //                             _ => {}
    //                         }
    //                         i += 1
    //                     }
    //                     Ok(LiveEvent::Common(message)) => {
    //                         //Ignoring comons for now
    //                         let mut text: heapless::String<256> = heapless::String::new();
    //                         write!(&mut text, "{:?}\n", message).ok();
    //                         c.local.uart_sender.try_send(text).ok();
    //                         i += 3;
    //                     }
    //                     Ok(LiveEvent::Midi { channel, message }) => {
    //                         c.shared.led.lock(|l| l.set_high().unwrap());
    //                         let mut text: heapless::String<256> = heapless::String::new();
    //                         write!(&mut text, "C:{} {:?}\n", u8::from(channel) + 1, message).ok();
    //                         c.local.uart_sender.try_send(text).ok();
    //                         c.local
    //                             .midi_sender
    //                             .try_send(MidiEvent {
    //                                 channel: u8::from(channel) + 1,
    //                                 message,
    //                             })
    //                             .ok();
    //                         i += 3;
    //                     }
    //                     Err(e) => {
    //                         let mut text: heapless::String<256> = heapless::String::new();
    //                         write!(&mut text, " {:?}", e).ok();
    //                         c.local.uart_sender.try_send(text).ok();
    //                         i += 1;
    //                     }
    //                 }
    //             }
    //         }
    //         Err(nb::Error::WouldBlock) => {
    //             c.local
    //                 .uart_sender
    //                 .try_send(heapless::String::from("Blocking"))
    //                 .ok();
    //         }
    //         Err(nb::Error::Other(hal::uart::ReadError { err_type, .. })) => {
    //             let mut uart_error: heapless::String<256> = heapless::String::from("Err: ");
    //             match err_type {
    //                 hal::uart::ReadErrorType::Overrun => {
    //                     uart_error.push_str("Overrun\n").ok()
    //                 }
    //                 hal::uart::ReadErrorType::Break => uart_error.push_str("Break\n").ok(),
    //                 hal::uart::ReadErrorType::Parity => uart_error.push_str("Parity\n").ok(),
    //                 hal::uart::ReadErrorType::Framing => {
    //                     uart_error.push_str("framing\n").ok()
    //                 }
    //             };
    //             c.local.uart_sender.try_send(uart_error).ok();
    //         }
    //     };
    // }

    #[task(priority = 1, shared = [], local = [watchdog])]
    async fn watchdog_feeder(c: watchdog_feeder::Context) {
        loop {
            c.local.watchdog.feed();
            Mono::delay(1000.micros()).await;
        }
    }

    // #[task(
    //     priority = 2,
    //     shared = [led],
    //     local = [usb_bus],
    // )]
    // async fn usb_handler(
    //     mut c: usb_handler::Context,
    //     mut receiver: MessageReceiver<heapless::String<256>>,
    // ) {
    //     let mut serial = usbd_serial::SerialPort::new(&c.local.usb_bus);
    //     let mut usb_dev = UsbDeviceBuilder::new(&c.local.usb_bus, UsbVidPid(0x16c0, 0x27dd))
    //         .manufacturer("Things")
    //         .product("Stuf")
    //         .serial_number("Whatever")
    //         .device_class(2)
    //         .build();

    //     c.shared.led.lock(|l| l.set_high().ok());
    //     while !usb_dev.poll(&mut [&mut serial]) {
    //         Timer::delay(2.millis()).await;
    //     }
    //     c.shared.led.lock(|l| l.set_low().ok());

    //     let clear: heapless::String<256> = heapless::String::from("Connected\n");
    //     let mut sent = false;
    //     while !sent {
    //         match serial.write(clear.as_bytes()) {
    //             Ok(_) => sent = true,
    //             Err(_) => (),
    //         }
    //         usb_dev.poll(&mut [&mut serial]);
    //         Timer::delay(5.millis()).await;
    //     }

    //     loop {
    //         match receiver.try_recv() {
    //             Ok(text) => match serial.write(text.as_bytes()) {
    //                 Ok(_) => 1,
    //                 Err(_) => 1,
    //             },
    //             _ => 0,
    //         };

    //         usb_dev.poll(&mut [&mut serial]);

    //         Timer::delay(7.millis()).await;
    //     }
    // }
}
