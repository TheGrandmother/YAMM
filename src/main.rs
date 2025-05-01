#![no_std]
#![no_main]
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unreachable_code)]

use panic_semihosting as _; // panic handler

mod button_handler;
mod commando_unit;
mod midi_mapper;
mod outs;
mod player;
mod prorgrammer;
mod pwm_pair;
mod utils;

use rtic_monotonics::rp2040::prelude::*;

rp2040_timer_monotonic!(Mono);

#[rtic::app(device = rp_pico::hal::pac, dispatchers = [SW0_IRQ, SW1_IRQ, SW2_IRQ, SW3_IRQ])]
mod midi_master {
    use ::nb::Error;
    use embedded_hal::can::nb;
    use embedded_hal::digital::v2::{InputPin, OutputPin};
    use fugit::Duration;
    use rp_pico::hal::{self, gpio};

    use fugit::RateExtU32;

    use hal::gpio::bank0::Gpio25;
    use hal::gpio::Pin;
    use rp_pico::hal::gpio::Interrupt;
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

    use hal::pwm;

    use crate::button_handler::ButtonHandler;
    use crate::commando_unit::{CommandEvent, CommandoUnit, Input, Operation};
    use crate::midi_mapper::{Config, MidiMapper};
    use crate::outs::{Cv, CvPorts, Gate, GateMappings, OutputHandler, OutputRequest};
    use crate::player::{Player, PlayerAction};
    use crate::prorgrammer::Programmer;
    use crate::pwm_pair::CvPair;
    use crate::utils::midi_size::event_length;
    use crate::Mono;

    const MESSAGE_CAPACITY: usize = 64;
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
        watchdog: hal::Watchdog,
        uart: UartType,
        midi_sender: MessageSender<LiveEvent<'static>>,
        output_handler: OutputHandler,
        midi_mapper: MidiMapper,
        commando_player_sender: MessageSender<PlayerAction>,
        uart_player_sender: MessageSender<PlayerAction>,
        uart_command_sender: MessageSender<CommandEvent>,
        player: Player,
        commando: CommandoUnit,
        button_handler: ButtonHandler,
        programmer: Programmer,
    }

    #[shared]
    struct Shared {
        led: gpio::Pin<DynPinId, gpio::FunctionSioOutput, gpio::PullDown>,
        output_sender: MessageSender<OutputRequest>,
        rec_switch: Pin<gpio::bank0::Gpio2, gpio::FunctionSioInput, gpio::PullUp>,
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
            open_hh: pins.gpio7.reconfigure(),
            clap: pins.gpio9.reconfigure(),
            snare: pins.gpio10.reconfigure(),
            kick: pins.gpio27.reconfigure(),
            fx: pins.gpio6.reconfigure(),
            accent: pins.gpio5.reconfigure(),
            closed_hh: pins.gpio8.reconfigure(),
            start: pins.gpio26.reconfigure(),
            stop: pins.gpio18.reconfigure(),
            clock: pins.gpio28.reconfigure(),
            gate_a: pins.gpio22.reconfigure(),
            gate_b: pins.gpio21.reconfigure(),
            gate_c: pins.gpio20.reconfigure(),
            gate_d: pins.gpio19.reconfigure(),
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

        let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
            c.device.USBCTRL_REGS,
            c.device.USBCTRL_DPRAM,
            clocks.usb_clock,
            true,
            &mut resets,
        ));

        // let mut play_btn = pins.gpio11.into_pull_up_input();
        // play_btn.set_interrupt_enabled(Interrupt::EdgeLow, true);
        // play_btn.set_interrupt_enabled(Interrupt::EdgeHigh, true);
        // play_btn.clear_interrupt(Interrupt::EdgeLow);
        // play_btn.clear_interrupt(Interrupt::EdgeHigh);

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

        let (midi_sender, midi_receiver) = make_channel!(LiveEvent<'static>, MESSAGE_CAPACITY);
        let (player_sender, player_receiver) = make_channel!(PlayerAction, MESSAGE_CAPACITY);
        let (commando_sender, command_receiver) = make_channel!(CommandEvent, MESSAGE_CAPACITY);

        let midi_mapper = MidiMapper::new(Config::two_mono(), output_sender.clone());
        let player = Player::new(0, 8, 4, midi_sender.clone(), output_sender.clone());
        let programmer = Programmer::new(player_sender.clone());

        let commando = CommandoUnit::new();
        let button_handler = ButtonHandler::new(
            pins.gpio11.reconfigure(),
            pins.gpio12.reconfigure(),
            pins.gpio13.reconfigure(),
            commando_sender.clone(),
        );

        watchdog.start(fugit::ExtU32::micros(50_000));
        watchdog_feeder::spawn().ok();
        // usb_handler::spawn(uart_receiver).ok();
        // test_suite::spawn(output_sender.clone()).ok();
        command_handler::spawn(command_receiver).ok();
        output_task::spawn(output_receiver).ok();
        midi_handler::spawn(midi_receiver).ok();
        player_handler::spawn(player_receiver).ok();

        return (
            Shared {
                led,
                output_sender: output_sender.clone(),
                rec_switch: pins.gpio2.reconfigure(),
            },
            Local {
                usb_bus,
                uart,
                watchdog,
                midi_sender,
                output_handler,
                midi_mapper,
                commando_player_sender: player_sender.clone(),
                uart_player_sender: player_sender.clone(),
                player,
                uart_command_sender: commando_sender.clone(),
                commando,
                button_handler,
                programmer,
            },
        );
    }

    #[task(priority=3, local = [output_handler], shared=[led])]
    async fn output_task(c: output_task::Context, mut receiver: MessageReceiver<OutputRequest>) {
        loop {
            match receiver.recv().await {
                Ok(req) => {
                    c.local.output_handler.handle_message(req);
                }
                Err(_) => {}
            }
        }
    }

    #[task(priority=2, local = [midi_mapper], shared=[])]
    async fn midi_handler(
        c: midi_handler::Context,
        mut receiver: MessageReceiver<LiveEvent<'static>>,
    ) {
        loop {
            match receiver.recv().await {
                Ok(event) => c.local.midi_mapper.handle_message(event).await,
                Err(_) => {}
            }
        }
    }

    #[task(priority=1, local = [commando_player_sender, commando, programmer], shared=[led, &rec_switch])]
    async fn command_handler(
        mut c: command_handler::Context,
        mut receiver: MessageReceiver<CommandEvent>,
    ) {
        loop {
            // FOR SOME HORRID REASON I CANNOT USE ASYNC HERE!?
            match receiver.try_recv() {
                Ok(event) if c.shared.rec_switch.is_high().unwrap_or(false) => {
                    match c.local.commando.handle_event(event) {
                        Some(op) => {
                            match op {
                                Operation::Begin(_) => {}
                                Operation::Commit => {
                                    c.shared.led.lock(|led| {
                                        if led.is_high().unwrap() {
                                            led.set_low().unwrap()
                                        } else {
                                            led.set_high().unwrap()
                                        }
                                    });
                                }
                                _ => {}
                            }
                            c.local.programmer.handle_operation(op)
                        }
                        None => {}
                    };
                }
                Err(_) => {}
                _ => {}
            }
            Mono::delay(1.millis()).await;
        }
    }

    #[task(priority=2, local = [player], shared=[led])]
    async fn player_handler(
        c: player_handler::Context,
        mut receiver: MessageReceiver<PlayerAction>,
    ) {
        loop {
            match receiver.recv().await {
                Ok(action) => c.local.player.handle_message(action),
                Err(_) => {}
            }
        }
    }

    #[task(local=[button_handler], shared=[led], binds=IO_IRQ_BANK0 )]
    fn gpio_handler(c: gpio_handler::Context) {
        c.local.button_handler.handle_irq()
    }

    #[task(local = [uart_player_sender, uart_command_sender, midi_sender, uart], shared=[led, &rec_switch], binds=UART0_IRQ)]
    fn uart(mut c: uart::Context) {
        let mut bob = [0u8; 256];
        if !c.local.uart.uart_is_readable() {
            return;
        }
        let recording = c.shared.rec_switch.is_high().unwrap();
        match c.local.uart.read_raw(&mut bob) {
            Ok(bytes) => {
                if bytes > 0 {
                    c.local.uart.write_raw(&bob).ok();
                }
                let mut bytes_consumed = 0;
                while bytes_consumed < bytes {
                    match LiveEvent::parse(&bob[bytes_consumed..]) {
                        Ok(event) => {
                            bytes_consumed += event_length(event);
                            if !recording {
                                c.local.midi_sender.try_send(event.to_static()).ok();
                            }
                            match event {
                                LiveEvent::Midi { message, .. } if recording => match message {
                                    MidiMessage::NoteOn { key, .. } => {
                                        c.local
                                            .uart_command_sender
                                            .try_send(CommandEvent::Down(Input::MidiKey(
                                                key.into(),
                                            )))
                                            .ok();
                                    }
                                    MidiMessage::NoteOff { key, .. } => {
                                        c.local
                                            .uart_command_sender
                                            .try_send(CommandEvent::Up(Input::MidiKey(key.into())))
                                            .ok();
                                    }
                                    _ => {}
                                },

                                LiveEvent::Realtime(msg) if !recording => match msg {
                                    midly::live::SystemRealtime::TimingClock => {
                                        c.local
                                            .uart_player_sender
                                            .try_send(PlayerAction::Tick)
                                            .ok();
                                    }
                                    midly::live::SystemRealtime::Start => {
                                        c.local
                                            .uart_player_sender
                                            .try_send(PlayerAction::Play)
                                            .ok();
                                    }
                                    midly::live::SystemRealtime::Stop => {
                                        c.local
                                            .uart_player_sender
                                            .try_send(PlayerAction::Stop)
                                            .ok();
                                    }
                                    _ => {}
                                },
                                _ => {}
                            }
                        }
                        Err(_) => {
                            // c.shared.led.lock(|led| {
                            //     if led.is_high().unwrap() {
                            //         led.set_low().unwrap()
                            //     } else {
                            //         led.set_high().unwrap()
                            //     }
                            // });
                            return;
                        }
                    }
                }
            }
            Err(Error::WouldBlock) => {}
            Err(Error::Other(_)) => {
                // c.shared.led.lock(|led| {
                //     if led.is_high().unwrap() {
                //         led.set_low().unwrap()
                //     } else {
                //         led.set_high().unwrap()
                //     }
                // });
            }
        };
    }

    #[task(priority = 4, shared = [], local = [watchdog])]
    async fn watchdog_feeder(c: watchdog_feeder::Context) {
        loop {
            c.local.watchdog.feed();
            Mono::delay(1_000.micros()).await;
        }
    }
}
