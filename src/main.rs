#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unreachable_code)]

mod pitched_channel;

#[rtic::app(device = rp_pico::hal::pac, peripherals = true, dispatchers = [SW0_IRQ, SW1_IRQ, SW2_IRQ])]
mod fuckall {
    use heapless::arc_pool;
    use panic_halt as _;

    use fugit::RateExtU32;

    use embedded_hal::digital::v2::OutputPin;

    use rp2040_hal as hal;
    use rp2040_hal::gpio::Interrupt::EdgeLow;
    use rp2040_hal::gpio::Interrupt::EdgeHigh;
    use rp2040_hal::gpio::PushPullOutput;

    use core::fmt::Write;

    use rtic_monotonics::rp2040::*;
    use rtic_monotonics::*;

    use usb_device;
    use usb_device::class_prelude::UsbBusAllocator;
    use usb_device::prelude::UsbDeviceBuilder;
    use usb_device::prelude::UsbVidPid;

    use midly::{live::LiveEvent, MidiMessage};

    use rtic_sync::{channel::*, make_channel};

    use rp2040_hal::gpio;
    use rp2040_hal::pwm;
    use embedded_hal::watchdog::WatchdogEnable;
    use embedded_hal::watchdog::Watchdog;
    use embedded_hal::PwmPin;
    use gpio::Pin;

    use crate::pitched_channel;
    use crate::pitched_channel::PitchedChannel;


    type MaybeUninit<X> = core::mem::MaybeUninit<X>;


    const MESSAGE_CAPACITY: usize = 16;
    type MessageSender<T> = Sender<'static, T, MESSAGE_CAPACITY>;
    type MessageReceiver<T> = Receiver<'static, T, MESSAGE_CAPACITY>;
    type UartType = hal::uart::UartPeripheral<
        hal::uart::Enabled,
        rp2040_hal::pac::UART0,
        (Pin<gpio::bank0::Gpio0,gpio::Function<gpio::Uart>>, Pin<gpio::bank0::Gpio1,gpio::Function<gpio::Uart>>)>;

    pub struct MidiEvent {
        channel: u8,
        message: MidiMessage
    }

    #[local]
    struct Local {
        usb_bus: UsbBusAllocator<hal::usb::UsbBus>,
        uart_sender: MessageSender<heapless::String<256>>,
        watchdog: hal::Watchdog,
        uart:UartType,
        midi_sender: MessageSender<MidiEvent>,
        voice_1_slice: hal::pwm::Slice<hal::pwm::Pwm7, pwm::FreeRunning>,
        voice_1_gate: gpio::Pin<gpio::pin::bank0::Gpio13, gpio::PushPullOutput>,
        button_pin: gpio::Pin<gpio::pin::bank0::Gpio18, gpio::Input<rp2040_hal::gpio::PullUp>>,
        clock_out: gpio::Pin<gpio::pin::bank0::Gpio19, gpio::PushPullOutput>,
        clock_high: bool
    }

    #[shared]
    struct Shared {
        led: gpio::Pin<gpio::pin::bank0::Gpio25, gpio::PushPullOutput>,
    }

    #[init()]
    fn init(c: init::Context) -> (Shared, Local) {
        unsafe {
            rp2040_hal::sio::spinlock_reset();
        }

        let mut resets = c.device.RESETS;

        let watchdog_timeout = c.device.WATCHDOG.reason.read().timer().bit_is_set();

        let mut watchdog = rp2040_hal::Watchdog::new(c.device.WATCHDOG);
        let clocks = rp2040_hal::clocks::init_clocks_and_plls(
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


        let sio = rp2040_hal::Sio::new(c.device.SIO);
        let pins = rp_pico::Pins::new(
            c.device.IO_BANK0,
            c.device.PADS_BANK0,
            sio.gpio_bank0,
            &mut resets,
        );

        let button_pin = pins.gpio18.into_pull_up_input();
        button_pin.set_interrupt_enabled(EdgeLow, true);
        button_pin.set_interrupt_enabled(EdgeHigh, true);


        let mut led = pins.led.into_push_pull_output();
        if watchdog_timeout {
            led.set_low().unwrap();
            let mut count = 100000;
            while count > 0 {count -= 1;}
            led.set_high().unwrap();
            count = 100000;
            while count > 0 {count -= 1;}
            led.set_low().unwrap();
        }
        watchdog.start(fugit::ExtU32::micros(300000));

        let clock_out = pins.gpio19.into_push_pull_output();

        let timer = c.device.TIMER;
        let token = rtic_monotonics::create_rp2040_monotonic_token!();
        Timer::start(timer, &mut resets, token);

        let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
            c.device.USBCTRL_REGS,
            c.device.USBCTRL_DPRAM,
            clocks.usb_clock,
            true,
            &mut resets,
        ));


        // Set up UART on GP0 and GP1 (Pico pins 1 and 2)
        let uart_pins = (
            pins.gpio0.into_mode::<gpio::FunctionUart>(),
            pins.gpio1.into_mode::<gpio::FunctionUart>(),
        );


        // Need to perform clock init before using UART or it will freeze.
        let mut uart = hal::uart::UartPeripheral::new(c.device.UART0, uart_pins, &mut resets).enable(
            hal::uart::UartConfig::new(31250.Hz(), hal::uart::DataBits::Eight, None, hal::uart::StopBits::One),
            hal::Clock::freq(&clocks.peripheral_clock),
        ).unwrap();
        uart.enable_rx_interrupt();




        let pwm_slices = hal::pwm::Slices::new(c.device.PWM, &mut resets);

        let mut voice_1_slice = pwm_slices.pwm7;
        voice_1_slice.set_div_int(1u8); // To set integer part of clock divider
        voice_1_slice.set_div_frac(0u8); //
        voice_1_slice.enable();
        voice_1_slice = voice_1_slice.into_mode::<hal::pwm::FreeRunning>();
        voice_1_slice.channel_a.output_to(pins.gpio14);
        voice_1_slice.channel_b.output_to(pins.gpio15);
        voice_1_slice.channel_b.clr_inverted();
        voice_1_slice.channel_a.clr_inverted();
        voice_1_slice.channel_a.set_duty(0x0fff);
        voice_1_slice.channel_b.set_duty(0x0fff);


        let mut voice_1_gate = pins.gpio13.into_push_pull_output();
        voice_1_gate.set_low().ok();


        let (uart_sender, uart_receiver) = make_channel!(heapless::String<256>, MESSAGE_CAPACITY);
        let (midi_sender, midi_receiver) = make_channel!(MidiEvent, MESSAGE_CAPACITY);

        watchdog_feeder::spawn().ok();
        usb_handler::spawn(uart_receiver).ok();
        ping_task::spawn(uart_sender.clone()).ok();
        midi_handler::spawn(midi_receiver).ok();

        return (
            Shared { led },
            Local {
                usb_bus,
                uart_sender: uart_sender.clone(),
                uart,
                watchdog,
                midi_sender,
                voice_1_slice,
                voice_1_gate,
                clock_out,
                clock_high: false,
                button_pin,
            },
        );
    }

    #[task(local = [], shared=[])]
    async fn ping_task(_cx: ping_task::Context, mut sender: MessageSender<heapless::String<256>>) {
        loop {
            Timer::delay(5000.millis()).await;
            let mut text: heapless::String<256> = heapless::String::new();
            writeln!(&mut text, "{:?}: ping", Timer::now().ticks()).ok();
            sender.send(text).await.ok();
        }
    }


    #[task(local = [voice_1_slice, voice_1_gate], shared=[])]
    async fn midi_handler(c: midi_handler::Context, mut receiver: MessageReceiver<MidiEvent>) {
        let mut thinger = |pitch: u16, vel: u16| {
            embedded_hal::PwmPin::set_duty(&mut c.local.voice_1_slice.channel_a, pitch);
            embedded_hal::PwmPin::set_duty(&mut c.local.voice_1_slice.channel_b, vel);
        };

        let mut set_gate = |val: bool| {
            if val {
                c.local.voice_1_gate.set_high().unwrap()
            } else {
                c.local.voice_1_gate.set_low().unwrap()
            }
        };

        let _chanell = PitchedChannel::new(
             0,
             &mut set_gate,
             &mut thinger
            );
        loop {
            match receiver.recv().await {
                Ok(MidiEvent {channel: 1, message}) => {
                    match message {
                        // MidiMessage::NoteOn {..} => {c.local.gate_pin.set_high().ok();}
                        // MidiMessage::NoteOff {..} => {c.local.gate_pin.set_low().ok();}
                        _ => {}
                    }
                }
                Ok(_) => {} // Ignore shit on other chanells
                Err(_) => {} // Errors are for then weak
            }
        }
    }

    #[task(local = [button_pin, clock_out], shared=[], binds=IO_IRQ_BANK0)]
    fn pin_task(cx: pin_task::Context) {
        if cx.local.button_pin.interrupt_status(EdgeLow) {
            cx.local.clock_out.set_low().ok();
            cx.local.button_pin.clear_interrupt(EdgeLow);
        }
        if cx.local.button_pin.interrupt_status(EdgeHigh) {
            cx.local.clock_out.set_high().ok();
            cx.local.button_pin.clear_interrupt(EdgeHigh);
        }
    }

    #[task(local = [clock_high, uart_sender, midi_sender, uart,], shared=[], binds=UART0_IRQ)]
    fn uart(c: uart::Context) {
        let mut bob = [0u8; 32];
        if !c.local.uart.uart_is_readable() {
            let _ = c.local.uart_sender.try_send(heapless::String::from("Shit aint readable"));
            return
        }
        match c.local.uart.read_raw(&mut bob) {
            Ok(bytes) => {
                if bytes > 0 {
                    c.local.uart.write_raw(&bob).ok();
                }
                let mut i = 0;
                while i < bytes {
                    match LiveEvent::parse(&bob[i ..]) {
                        Ok(LiveEvent::Realtime(message)) => {
                            // Ignoring Clocks and such for now
                            match message {
                                midly::live::SystemRealtime::TimingClock => {
                                    // if *c.local.clock_high {
                                    //     c.local.clock_out.set_low().ok();
                                    //     *c.local.clock_high = false;
                                    // } else {
                                    //     c.local.clock_out.set_high().ok();
                                    //     *c.local.clock_high = true;
                                    // }
                                    // clock_ticker::spawn().ok();
                                }
                                _ => {}
                            }
                            i+=1
                        },
                        Ok(LiveEvent::Common(message)) => {
                            //Ignoring comons for now
                            let mut text: heapless::String<256> = heapless::String::new();
                            write!(&mut text, "{:?}\n", message).ok();
                            c.local.uart_sender.try_send(text).ok();
                            i+=3;
                        }
                        Ok(LiveEvent::Midi {channel, message}) => {
                            let mut text: heapless::String<256> = heapless::String::new();
                            write!(&mut text, "{:?}\n", message).ok();
                            c.local.uart_sender.try_send(text).ok();
                            c.local.midi_sender.try_send(MidiEvent {channel: u8::from(channel) + 1, message}).ok();
                            i+=3;
                        }
                        Err(e) => {
                            let mut text: heapless::String<256> = heapless::String::new();
                            write!(&mut text, " {:?}", e).ok();
                            c.local.uart_sender.try_send(text).ok();
                            i+=1;
                        }
                    }
                }
            },
            Err(nb::Error::WouldBlock) => {
                c.local.uart_sender.try_send(heapless::String::from("Blocking")).ok();
            },
            Err(nb::Error::Other(rp2040_hal::uart::ReadError{err_type, ..})) => {
                let mut uart_error: heapless::String<256> = heapless::String::from("Err: ");
                match err_type {
                    rp2040_hal::uart::ReadErrorType::Overrun => {uart_error.push_str("Overrun\n").ok()},
                    rp2040_hal::uart::ReadErrorType::Break => {uart_error.push_str("Break\n").ok()},
                    rp2040_hal::uart::ReadErrorType::Parity => {uart_error.push_str("Parity\n").ok()},
                    rp2040_hal::uart::ReadErrorType::Framing => {uart_error.push_str("framing\n").ok()},
                };
                c.local.uart_sender.try_send(uart_error).ok();
            },
        };
    }

    //#[task(priority = 2, shared = [], local = [clock_out])]
    //async fn clock_ticker(c: clock_ticker::Context) {
    //    c.local.clock_out.set_high().ok();
    //    Timer::delay(100.micros()).await;
    //    c.local.clock_out.set_low().ok();
    //}

    #[task(priority = 0, shared = [], local = [watchdog])]
    async fn watchdog_feeder(c: watchdog_feeder::Context) {
        loop {
            c.local.watchdog.feed();
            Timer::delay(10000.micros()).await;
        }
    }

    #[task(
        priority = 1,
        shared = [led],
        local = [usb_bus],
    )]
    async fn usb_handler(c: usb_handler::Context, mut receiver: MessageReceiver<heapless::String<256>>) {
        let mut serial = usbd_serial::SerialPort::new(&c.local.usb_bus);
        let mut usb_dev = UsbDeviceBuilder::new(&c.local.usb_bus, UsbVidPid(0x16c0, 0x27dd))
            .manufacturer("Things")
            .product("Stuf")
            .serial_number("Whatever")
            .device_class(2)
            .build();

        Timer::delay(1500.millis()).await;

        let clear: heapless::String<256> = heapless::String::from("Connected");
        serial.write(clear.as_bytes()).ok();

        loop {
            match receiver.try_recv() {
                Ok(text) => match serial.write(text.as_bytes()) {
                    Ok(_) => 1,
                    Err(_) => {
                        1
                    }
                },
                _ => 0,
            };

            usb_dev.poll(&mut [&mut serial]);

            Timer::delay(7.millis()).await;
        }
    }
}
