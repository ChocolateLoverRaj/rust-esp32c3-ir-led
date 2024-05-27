use std::{
    ops::Add,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use esp_idf_hal::gpio::{
    AnyIOPin, Gpio7, Gpio10, Gpio8, IOPin, Input, InterruptType, Level, Output, PinDriver, Pull,
};
use futures::{
    future::{select, Either},
    Future, StreamExt,
};
use common::ir_data::IrData;

use crate::subscribable2::Subscribable2;

pub struct IrSensor {
    led_pin: PinDriver<'static, Gpio10, Output>,
    receiver_pin: PinDriver<'static, AnyIOPin, Input>,
}
impl IrSensor {
    pub fn turn_on_and_check_is_receiving_light(&mut self) -> bool {
        self.led_pin.set_high().unwrap();
        // Let the light shine, idk how long it takes
        thread::sleep(Duration::from_millis(10));
        let is_receiving_light = self.receiver_pin.is_low();
        self.led_pin.set_low().unwrap();
        is_receiving_light
    }

    pub fn set_light(&mut self, on: bool) {
        self.led_pin
            .set_level(match on {
                true => Level::High,
                false => Level::Low,
            })
            .unwrap();
    }

    pub fn is_receiving_light(&self) -> bool {
        self.receiver_pin.is_low()
    }
}

pub fn configure_and_get_ir_sensor(led_pin: Gpio10, receiver_gpio: Gpio7) -> Option<IrSensor> {
    let mut led_pin: PinDriver<'_, Gpio10, Input> = PinDriver::input(led_pin).unwrap();
    led_pin.set_pull(Pull::Up).unwrap();
    if led_pin.is_low() {
        let led_pin = led_pin.into_output().unwrap();
        let mut receiver_pin = PinDriver::input(receiver_gpio.downgrade()).unwrap();
        receiver_pin.set_pull(Pull::Down).unwrap();
        receiver_pin
            .set_interrupt_type(InterruptType::AnyEdge)
            .unwrap();
        receiver_pin.enable_interrupt().unwrap();

        Some(IrSensor {
            led_pin,
            receiver_pin,
        })
    } else {
        None
    }
}

pub type IrSubscribable = Subscribable2<IrData>;

pub fn ir_loop(
    ir_sensor: Arc<Mutex<IrSensor>>,
    gpio8: Gpio8,
) -> (impl Future<Output = ()>, IrSubscribable) {
    let (subscribable, mut rx) = Subscribable2::new();
    (
        {
            let mut subscribable = subscribable.clone();
            async move {
                // Initialize Pin 8 as an output to drive the built-in LED (just as a secondary way of knowing)
                let mut secondary_led_pin = PinDriver::output(gpio8).unwrap();

                let mut previous = None::<bool>;
                rx.next().await.unwrap();
                log::warn!("Starting ir loop");
                loop {
                    let mut ir_sensor_guard = ir_sensor.lock().unwrap();
                    ir_sensor_guard.set_light(true);
                    let is_receiving_light = ir_sensor_guard.is_receiving_light();
                    if is_receiving_light {
                        secondary_led_pin.set_low().unwrap();
                    } else {
                        secondary_led_pin.set_high().unwrap();
                    }
                    if previous.map_or(true, |previous| is_receiving_light != previous) {
                        subscribable.update(IrData {
                            is_receiving_light,
                            time: SystemTime::now(),
                        });
                    }
                    previous = Some(is_receiving_light);
                    drop(ir_sensor_guard);
                    let r = select(
                        rx.next(),
                        Box::pin(async {
                            ir_sensor
                                .lock()
                                .unwrap()
                                .receiver_pin
                                .wait_for_any_edge()
                                .await
                                .unwrap()
                        }),
                    )
                    .await;
                    match r {
                        Either::Left((option, future)) => {
                            option.unwrap();

                            // FIXME: LED stays on until receiver input changes
                            future.await;
                            ir_sensor.lock().unwrap().set_light(false);
                            // Wait to resume again
                            rx.next().await.unwrap();
                            ir_sensor.lock().unwrap().set_light(true);
                        }
                        Either::Right(_) => {}
                    }
                }
            }
        },
        subscribable,
    )
}
