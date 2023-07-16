#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

#[rtic::app(device = rp_pico::hal::pac, peripherals = true, dispatchers = [SW0_IRQ])]
mod fuckall {
    use panic_halt as _;

    use embedded_hal::digital::v2::OutputPin;

    use rp2040_hal as hal;
    use rp2040_hal::gpio::Interrupt::EdgeLow;

    use core::fmt::Write;

    use rtic_monotonics::rp2040::*;
    use rtic_monotonics::*;

    use usb_device;
    use usb_device::class_prelude::UsbBusAllocator;
    use usb_device::prelude::UsbDeviceBuilder;
    use usb_device::prelude::UsbVidPid;

    use rtic_sync::{channel::*, make_channel};

    use rp2040_hal::gpio;

    const MESSAGE_CAPACITY: usize = 5;
    type MessageSender = Sender<'static, heapless::String<128>, MESSAGE_CAPACITY>;
    type MessageReceiver = Receiver<'static, heapless::String<128>, MESSAGE_CAPACITY>;

    #[local]
    struct Local {
        usb_bus: UsbBusAllocator<hal::usb::UsbBus>,
        button_pin: gpio::Pin<gpio::pin::bank0::Gpio16, gpio::Input<gpio::PullUp>>,
    }

    #[shared]
    struct Shared {
        led: gpio::Pin<gpio::pin::bank0::Gpio25, gpio::PushPullOutput>,
    }

    #[init]
    fn init(c: init::Context) -> (Shared, Local) {
        unsafe {
            rp2040_hal::sio::spinlock_reset();
        }

        let mut resets = c.device.RESETS;
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

        let button_pin = pins.gpio16.into_pull_up_input();
        button_pin.set_interrupt_enabled(EdgeLow, true);

        let mut led = pins.led.into_push_pull_output();
        led.set_low().unwrap();

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

        let (s, r) = make_channel!(heapless::String<128>, MESSAGE_CAPACITY);

        usb_handler::spawn(r).ok();
        ping_task::spawn(s.clone()).ok();

        return (
            Shared { led },
            Local {
                usb_bus,
                button_pin,
            },
        );
    }

    #[task(local = [], shared=[])]
    async fn ping_task(_cx: ping_task::Context, mut sender: MessageSender) {
        loop {
            Timer::delay(1000.millis()).await;
            let mut text: heapless::String<128> = heapless::String::new();
            writeln!(&mut text, "{:?}: ping", Timer::now().ticks()).unwrap();
            sender.send(text).await.unwrap();
        }
    }

    #[task(local = [button_pin], shared=[], binds=IO_IRQ_BANK0)]
    fn pin_task(cx: pin_task::Context) {
        blink::spawn().ok();
        if cx.local.button_pin.interrupt_status(EdgeLow) {
            cx.local.button_pin.clear_interrupt(EdgeLow);
        }
    }

    #[task(local = [], shared=[led])]
    async fn blink(mut cx: blink::Context) {
        cx.shared.led.lock(|l| l.set_low().unwrap());
        cx.shared.led.lock(|l| l.set_high().unwrap());
        Timer::delay(10000.millis()).await;
        cx.shared.led.lock(|l| l.set_low().unwrap());
    }

    #[task(
        priority = 1,
        shared = [led],
        local = [usb_bus],
    )]
    async fn usb_handler(mut c: usb_handler::Context, mut receiver: MessageReceiver) {
        let mut serial = usbd_serial::SerialPort::new(&c.local.usb_bus);
        let mut usb_dev = UsbDeviceBuilder::new(&c.local.usb_bus, UsbVidPid(0x16c0, 0x27dd))
            .manufacturer("Things")
            .product("Stuf")
            .serial_number("Whatever")
            .device_class(2) // from: https://www.usb.org/defined-class-codes
            .build();

        Timer::delay(100.millis()).await;
        c.shared.led.lock(|l| l.set_high().unwrap());
        Timer::delay(150.millis()).await;
        c.shared.led.lock(|l| l.set_low().unwrap());

        let clear: heapless::String<128> = heapless::String::from("Connected");
        serial.write(clear.as_bytes()).unwrap();

        loop {
            match receiver.try_recv() {
                Ok(text) => match serial.write(text.as_bytes()) {
                    Ok(_) => 1,
                    Err(_) => {
                        c.shared.led.lock(|l| l.set_high().unwrap());
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
