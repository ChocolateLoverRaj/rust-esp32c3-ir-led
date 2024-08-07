use std::time::Duration;

use crate::{
    config::{
        DEVICE_NAME, IGNORE_TV_POWER_STATE, SHOULD_CONTROL_SOUND_SYSTEM, SHOULD_CONTROL_TV,
        SHOULD_SWITCH_SOUND_OUTPUT, TV_MAC_ADDRESS,
    },
    get_wakeup_reason::get_wakeup_reason,
    samsung::Samsung,
    sound_system::SoundSystem,
    toggle_game_mode::toggle_game_mode,
    tv_data::{get_tv_data, save_tv_data},
};
use smart_power_button_common::WakeupReason;
use tokio::{join, time::sleep};
use wakey::WolPacket;

pub async fn power_up() -> anyhow::Result<()> {
    let tv_data_future = get_tv_data();
    let wakeup_reason_future = get_wakeup_reason();
    let (tv_data, wakeup_reason) = join!(tv_data_future, wakeup_reason_future);
    let mut tv_data = tv_data?.unwrap_or_default();
    println!("Read TV Data: {:#?}", tv_data);
    let mut wakeup_reason = wakeup_reason?;
    println!("Wakeup reason: {wakeup_reason:?}");
    if IGNORE_TV_POWER_STATE {
        wakeup_reason = Some(WakeupReason::Web(true));
    }
    let should_turn_on_tv = match wakeup_reason {
        Some(WakeupReason::Bluetooth(_)) => true,
        Some(WakeupReason::Web(should_turn_on_tv)) => should_turn_on_tv,
        None => true,
    };
    if should_turn_on_tv && !tv_data.is_on || IGNORE_TV_POWER_STATE {
        println!("Should turn on tv");
        let sound_system_future = async {
            if SHOULD_CONTROL_SOUND_SYSTEM {
                println!("Turning on sound system");
                SoundSystem::open().await?.turn_on().await?;
                println!("Done turning on sound system");
            }
            Ok::<_, anyhow::Error>(())
        };

        let tv_future = async {
            if SHOULD_CONTROL_TV {
                // Turn on
                println!("Turning TV on");
                WolPacket::from_string(TV_MAC_ADDRESS, ':')?.send_magic()?;
                let mut remote = Samsung {
                    ip: "samsung.local".into(),
                    app_name: "Gaming Computer".into(),
                    token: tv_data.token.clone(),
                };
                remote.send_key("KEY_HOME").await?;
                sleep(Duration::from_secs_f64(1.0)).await;
                // Move all the way left
                for _ in 0..11 {
                    remote.send_key("KEY_LEFT").await?;
                    sleep(Duration::from_secs_f64(0.15)).await;
                }
                if SHOULD_SWITCH_SOUND_OUTPUT {
                    // Switch the sound output from TV to Sound System
                    remote.send_key("KEY_UP").await?;
                    remote.send_key("KEY_RIGHT").await?;
                    remote.send_key("KEY_RIGHT").await?;
                    remote.send_key("KEY_ENTER").await?;
                    // Go back to home settings
                    remote.send_key("KEY_DOWN").await?;
                }

                // Go to source settings
                remote.send_key("KEY_RIGHT").await?;
                remote.send_key("KEY_UP").await?;
                // Move all the way to the left
                for _ in 0..5 {
                    remote.send_key("KEY_LEFT").await?;
                    sleep(Duration::from_secs_f64(0.15)).await;
                }
                // Switch to HDMI1, and change it to be "Game Console" type
                // Select HDMI1
                remote.send_key("KEY_RIGHT").await?;
                // Go up to "Choose type"
                remote.send_key("KEY_UP").await?;
                remote.send_key("KEY_UP").await?;
                remote.send_key("KEY_ENTER").await?;
                sleep(Duration::from_secs_f64(1.5)).await;
                // Go all the way up in case it's already set to "Game Console"
                for _ in 0..4 {
                    remote.send_key("KEY_UP").await?;
                    sleep(Duration::from_secs_f64(0.9)).await;
                }
                // Go down to "Game Console"
                for _ in 0..2 {
                    remote.send_key("KEY_DOWN").await?;
                    sleep(Duration::from_secs_f64(0.5)).await;
                }
                // Select "Game Console"
                remote.send_key("KEY_ENTER").await?;
                sleep(Duration::from_secs_f64(0.2)).await;
                // Go right to edit the name
                remote.send_key("KEY_RIGHT").await?;
                // Click on the name to edit it
                remote.send_key("KEY_ENTER").await?;
                sleep(Duration::from_secs_f64(1.9)).await;
                remote.send_text(DEVICE_NAME).await?;
                sleep(Duration::from_secs_f64(1.9)).await;
                // Exit the typing
                remote.send_key("KEY_RETURN").await?;
                sleep(Duration::from_secs_f64(0.9)).await;
                // Go down to the "OK" button
                remote.send_key("KEY_DOWN").await?;
                sleep(Duration::from_secs_f64(0.5)).await;
                // Press the "OK" button
                remote.send_key("KEY_ENTER").await?;
                sleep(Duration::from_secs_f64(2.0)).await;
                // Switch to HDMI1
                remote.send_key("KEY_ENTER").await?;
                // sleep(Duration::MAX).await;
                // The TV will show the "Detecting device" spinner. Cancel the spinner
                sleep(Duration::from_secs_f64(3.0)).await;
                remote.send_key("KEY_RETURN").await?;
                sleep(Duration::from_secs_f64(1.0)).await;
                // Turn game mode on
                toggle_game_mode(&mut remote).await?;
                remote.send_key("KEY_HOME").await?;

                tv_data.token = remote.token
            }
            Ok::<_, anyhow::Error>(())
        };
        {
            let (r1, r2) = join!(sound_system_future, tv_future);
            r1?;
            r2?;
        }

        tv_data.is_on = true;
        println!("Saving TV Data: {:#?}", tv_data);
        save_tv_data(&tv_data).await?;
    }
    Ok(())
}
