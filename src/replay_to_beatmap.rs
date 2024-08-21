use std::collections::VecDeque;

use osu_db::Replay;
use osu_file_parser::OsuFile;
use rust_decimal::prelude::*;

#[derive(Debug)]
struct Click {
    time: i64,
    x: f32,
    y: f32,
}

pub fn convert_replay_to_beatmap(mut beatmap: OsuFile, replay: &Replay) -> OsuFile {
    let replay_data = replay.replay_data.as_ref().expect("Missing replay data");
    let mut clicks = VecDeque::new();

    let mut time = 0;
    for i in 1..replay_data.len() {
        let action = &replay_data[i];
        let action_btns = action.std_buttons().0;
        let last_action_btns = replay_data[i - 1].std_buttons().0;

        time += action.delta;

        if (!last_action_btns & action_btns & 3) > 0 {
            clicks.push_back(Click {
                time,
                x: action.x,
                y: action.y,
            });
        }
    }

    let hitobjects = &mut beatmap
        .hitobjects
        .as_mut()
        .expect("This beatmap doesn't have any hitobjects")
        .0;

    for hitobject in hitobjects.iter_mut() {
        let time = hitobject.time.get().clone().unwrap_left().to_i64().unwrap(); // this man needs to ......nm HIMSELF
        let click_idx = {
            match clicks.binary_search_by_key(&time, |v| v.time) {
                Ok(i) => i,
                Err(i) => i,
            }
        };

        if click_idx == clicks.len() {
            break;
        }

        let click_idx = {
            if click_idx != 0 {
                let click_after = &clicks.get(click_idx).map(|e| e.time).unwrap_or(i64::MAX);
                let click_before = &clicks
                    .get(click_idx - 1)
                    .map(|e| e.time)
                    .unwrap_or(i64::MIN);
                if click_idx > 0 && time - click_before < click_after - time {
                    click_idx - 1
                } else {
                    click_idx
                }
            } else {
                0
            }
        };

        let click = clicks.get(click_idx).unwrap();
        hitobject.time = (click.time as i32).into(); // XD
        hitobject.position.x = (click.x as i32).into();
        hitobject.position.y = (click.y as i32).into();

        clicks.drain(..click_idx + 1);
    }

    return beatmap;
}
