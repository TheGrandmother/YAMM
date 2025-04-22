#![no_std]
#![no_main]
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unreachable_code)]

use panic_semihosting as _; // panic handler

mod midi_mapper;
mod outs;
mod pwm_pair;

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
    use rp_pico::hal::gpio::PullDown;
    use rp_pico::hal::gpio::SioOutput;
    use rp_pico::hal::gpio::{DynPinId, FunctionSio};
    use usb_device::device::StringDescriptors;

    use core::fmt::Write;
    use rtic_monotonics::rp2040::prelude::*;

    use usb_device::class_prelude::UsbBusAllocator;
    use usb_device::prelude::UsbDeviceBuilder;
    use usb_device::prelude::UsbVidPid;
    use usb_device::{self, LangID};

    use midly::{live::LiveEvent, MidiMessage};

    use rtic_sync::{channel::*, make_channel};

    use hal::gpio;
    use hal::pwm;

    use crate::midi_mapper::{Config, MidiMapper};
    use crate::outs::{Cv, CvPorts, Gate, GateMappings, OutputHandler, OutputRequest};
    use crate::pwm_pair::CvPair;
    use crate::Mono;

    const MESSAGE_CAPACITY: usize = 16;
    const PITCHED_CHANELL: midly::num::u4 = midly::num::u4::new(1);
    const DRUM_CHANELL: midly::num::u4 = midly::num::u4::new(5);
    pub type MessageSender<T> = Sender<'static, T, MESSAGE_CAPACITY>;
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
        led: &mut Pin<DynPinId, FunctionSio<SioOutput>, PullDown>,
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
        usb_bus: UsbBusAllocator<hal::usb::UsbBus>,
        uart_sender: MessageSender<heapless::String<256>>,
        watchdog: hal::Watchdog,
        uart: UartType,
        midi_sender: MessageSender<LiveEvent<'static>>,
        clock_high: bool,
        divide_clock: bool,
        output_handler: OutputHandler,
        midi_mapper: MidiMapper,
    }

    #[shared]
    struct Shared {
        led: gpio::Pin<DynPinId, gpio::FunctionSioOutput, gpio::PullDown>,
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

        let mut led = pins.gpio25.into_push_pull_output().into_dyn_pin();
        led.set_high().unwrap();
        if watchdog_timeout {
            led.set_low().unwrap();
            let mut blinks = 5;
            while blinks > 0 {
                blink(&mut led, 100.millis(), true);
                blinks -= 1;
            }
        }

        let gate_pins = GateMappings {
            open_hh: pins.gpio7.into_push_pull_output(),
            clap: pins.gpio9.into_push_pull_output(),
            snare: pins.gpio10.into_push_pull_output(),
            bd: pins.gpio27.into_push_pull_output(),
            fx: pins.gpio6.into_push_pull_output(),
            accent: pins.gpio5.into_push_pull_output(),
            closed_hh: pins.gpio8.into_push_pull_output(),
            start: pins.gpio26.into_push_pull_output(),
            stop: pins.gpio18.into_push_pull_output(),
            clock: pins.gpio28.into_push_pull_output(),
            gate_a: pins.gpio22.into_push_pull_output(),
            gate_b: pins.gpio21.into_push_pull_output(),
            gate_c: pins.gpio20.into_push_pull_output(),
            gate_d: pins.gpio19.into_push_pull_output(),
        };

        let pwm_slices = hal::pwm::Slices::new(c.device.PWM, &mut resets);

        let cv_ports = CvPorts {
            ab_pair: CvPair::new(
                pwm_slices.pwm7,
                pins.gpio14.into_function::<gpio::FunctionPwm>(),
                pins.gpio15.into_function::<gpio::FunctionPwm>(),
            ),
            cd_pair: CvPair::new(
                pwm_slices.pwm0,
                pins.gpio16.into_function::<gpio::FunctionPwm>(),
                pins.gpio17.into_function::<gpio::FunctionPwm>(),
            ),
        };

        let mut output_handler = OutputHandler::new(gate_pins, cv_ports);
        output_handler.reset();

        let (output_sender, output_receiver) = make_channel!(OutputRequest, MESSAGE_CAPACITY);

        let mut midi_mapper = MidiMapper::new(Config::two_duo(), output_sender.clone());

        let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
            c.device.USBCTRL_REGS,
            c.device.USBCTRL_DPRAM,
            clocks.usb_clock,
            true,
            &mut resets,
        ));

        let divide_clock = pins.gpio2.into_pull_up_input().is_high().unwrap();

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

        let (uart_sender, uart_receiver) = make_channel!(heapless::String<256>, MESSAGE_CAPACITY);
        let (midi_sender, midi_receiver) = make_channel!(LiveEvent, MESSAGE_CAPACITY);

        watchdog.start(fugit::ExtU32::micros(10_000));
        watchdog_feeder::spawn().ok();
        usb_handler::spawn(uart_receiver).ok();
        // test_suite::spawn(output_sender.clone() /*, uart_sender.clone()*/).ok();
        output_task::spawn(output_receiver /*, uart_sender.clone()*/).ok();

        return (
            Shared { led },
            Local {
                usb_bus,
                uart_sender: uart_sender.clone(),
                uart,
                watchdog,
                midi_sender,
                clock_high: false,
                divide_clock,
                output_handler,
                midi_mapper,
            },
        );
    }

    #[task(priority=0, shared = [])]
    async fn test_suite(_c: test_suite::Context, mut output_sender: MessageSender<OutputRequest>) {
        // uart_sender.try_send(heapless::String::from("Testing")).ok();
        let things = [
            Gate::BD,
            Gate::Snare,
            Gate::Clap,
            Gate::ClosedHH,
            Gate::OpenHH,
            Gate::FX,
            Gate::Accent,
            Gate::GateA,
            Gate::GateB,
            Gate::GateC,
            Gate::GateD,
            Gate::Clock,
            Gate::Start,
            Gate::Stop,
        ];
        let mut i = 0;
        let max = 14;
        let channels = [Cv::A, Cv::B, Cv::C, Cv::D];
        let mut channel = 0;
        let mut note = 12;
        loop {
            if note > 12 * 6 {
                note = 12;
                channel += 1;
            }
            if channel == 4 {
                output_sender
                    .try_send(OutputRequest::SetVal(channels[0], 0.0))
                    .ok();
                output_sender
                    .try_send(OutputRequest::SetVal(channels[1], 0.0))
                    .ok();
                output_sender
                    .try_send(OutputRequest::SetVal(channels[2], 0.0))
                    .ok();
                output_sender
                    .try_send(OutputRequest::SetVal(channels[3], 0.0))
                    .ok();
                channel = 0
            }

            output_sender
                .try_send(OutputRequest::SetNote(channels[channel], note))
                .ok();

            note += 3;

            output_sender
                .try_send(OutputRequest::GateOff(things[i]))
                .ok();
            output_sender
                .try_send(OutputRequest::GateOn(things[(i + 1) % 14]))
                .ok();
            output_sender
                .try_send(OutputRequest::GateOn(things[(i + 2) % 14]))
                .ok();
            output_sender
                .try_send(OutputRequest::GateOn(things[(i + 3) % 14]))
                .ok();
            output_sender
                .try_send(OutputRequest::GateOn(things[(i + 4) % 14]))
                .ok();
            i += 1;
            if i == max {
                i = 0;
            }
            Mono::delay(300_00.micros()).await;
        }
    }

    #[task(local = [output_handler], shared=[])]
    async fn output_task(c: output_task::Context, mut receiver: MessageReceiver<OutputRequest>) {
        loop {
            match receiver.recv().await {
                Ok(req) => c.local.output_handler.handle_message(req).unwrap(),
                Err(_) => {}
            }
        }
    }

    #[task(local = [midi_mapper, clock_high, uart_sender, midi_sender, uart], shared=[led], binds=UART0_IRQ)]
    fn uart(mut c: uart::Context) {
        let mut bob = [0u8; 32];
        if !c.local.uart.uart_is_readable() {
            let _ = c.local.uart_sender.try_send(heapless::String::from(
                heapless::String::try_from("aint readable").unwrap(),
            ));
            return;
        }
        match c.local.uart.read_raw(&mut bob) {
            Ok(bytes) => {
                if bytes > 0 {
                    c.local.uart.write_raw(&bob).ok();
                }
                let mut bytes_consumed = 0;
                while bytes_consumed < bytes {
                    match LiveEvent::parse(&bob[bytes_consumed..]) {
                        Ok(event) => {
                            match event {
                                LiveEvent::Realtime(_) => bytes_consumed += 1,
                                _ => bytes_consumed += 3,
                            }
                            c.local.midi_mapper.handle_message(event)
                        }
                        Err(_) => {
                            bytes_consumed += 1;
                            c.shared.led.lock(|led| {
                                if led.is_high().unwrap() {
                                    led.set_low().unwrap()
                                } else {
                                    led.set_high().unwrap()
                                }
                            });
                        }
                    }
                }
            }
            Err(Error::WouldBlock) => {}
            Err(Error::Other(_)) => {
                c.shared.led.lock(|led| {
                    if led.is_high().unwrap() {
                        led.set_low().unwrap()
                    } else {
                        led.set_high().unwrap()
                    }
                });
            }
        };
    }

    #[task(priority = 2, shared = [], local = [watchdog])]
    async fn watchdog_feeder(c: watchdog_feeder::Context) {
        loop {
            c.local.watchdog.feed();
            Mono::delay(1000.micros()).await;
        }
    }

    #[task(
        priority = 1,
        shared = [led],
        local = [usb_bus],
    )]
    async fn usb_handler(
        mut c: usb_handler::Context,
        mut receiver: MessageReceiver<heapless::String<256>>,
    ) {
        let mut serial = usbd_serial::SerialPort::new(&c.local.usb_bus);
        let mut usb_dev = UsbDeviceBuilder::new(&c.local.usb_bus, UsbVidPid(0x16c0, 0x27dd))
            .strings(&[StringDescriptors::new(LangID::EN)
                .manufacturer("Symbolic Circuits")
                .product("Master module")])
            .expect("Failed to set strings")
            .device_class(2)
            .build();

        c.shared.led.lock(|l| l.set_high().ok());
        while !usb_dev.poll(&mut [&mut serial]) {
            Mono::delay(2000.micros()).await;
        }
        c.shared.led.lock(|l| l.set_low().ok());

        let clear: heapless::String<256> = heapless::String::try_from("Mjau").unwrap();
        let mut sent = false;
        while !sent {
            match serial.write(clear.as_bytes()) {
                Ok(_) => sent = true,
                Err(_) => (),
            }
            usb_dev.poll(&mut [&mut serial]);
            Mono::delay(5000.micros()).await;
        }

        c.shared.led.lock(|l| l.set_high().ok());
        Mono::delay(1000.millis()).await;
        c.shared.led.lock(|l| l.set_low().ok());

        loop {
            match receiver.try_recv() {
                Ok(text) => match serial.write(text.as_bytes()) {
                    Ok(_) => 1,
                    Err(_) => 1,
                },
                _ => 0,
            };

            usb_dev.poll(&mut [&mut serial]);

            Mono::delay(7.millis()).await;
        }
    }
}
