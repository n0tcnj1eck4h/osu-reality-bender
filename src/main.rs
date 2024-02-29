use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Write};
use std::{env::args_os, path::Path};

use osu_db::listing::Listing;
use osu_db::replay::Replay;

use osu_file_parser::OsuFile;
use rust_decimal::prelude::ToPrimitive;

#[derive(Debug)]
struct Click {
    time: i64,
    x: f32,
    y: f32,
}

fn main() {
    let osu_path = std::env::var("OSU_PATH").unwrap_or(".".to_string());
    let osu_path = Path::new(&osu_path);

    let replay_path = args_os().nth(1).expect("nnghh (incorrect usage)");
    let mut replay = Replay::from_file(replay_path).expect("Failed to read/parse the replay");

    let mut listing = Listing::from_file(osu_path.join("osu!.db"))
        .expect("osu!.db not found. set your osu path with le OSU_PATH envirionment variable or run the program in your osu folder");

    let listing_map = listing
        .beatmaps
        .iter()
        .find(|e| e.hash == replay.beatmap_hash)
        .expect("Couldn't find the beatmap in osu!.db");

    let folder_name = listing_map
        .folder_name
        .as_ref()
        .expect("Beatmap listing is missing its folder name");

    let file_name = listing_map
        .file_name
        .as_ref()
        .expect("Beatmap listing is missing its file name");

    let mut beatmap = {
        let mut beatmap_path = osu_path.join("Songs");
        beatmap_path.push(folder_name);
        beatmap_path.push(file_name);

        let mut string = String::new();
        let mut file = File::open(beatmap_path).expect("Failed to open osu file");
        file.read_to_string(&mut string).unwrap();

        string.parse::<OsuFile>().expect("Failed to parse osu file")
    };

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

        // println!("{} {}: {:?}", click_idx, time, click.map(|e| e.time));
    }

    let mut new_osu_file_path = osu_path.join("Songs");

    new_osu_file_path.push(folder_name);
    new_osu_file_path.push(format!("awesome {}", file_name));

    let beatmap_string = beatmap.to_string();
    let mut out_file = File::create(&new_osu_file_path).expect("Failed to create osu file");
    write!(out_file, "{}", beatmap_string).expect("Failed to write the osu file");

    let new_hash = format!("{:x}", md5::compute(beatmap_string));

    println!(
        "osu file written to {} with hash {}",
        new_osu_file_path.to_string_lossy(),
        &new_hash
    );

    replay.beatmap_hash = Some(new_hash);
    replay
        .save("awesomereplay.osr", Some(2))
        .expect("Failed to save replay");

    println!("replay file written to awesomereplay.osr");

    let mut new_listing_map = listing_map.clone();
    new_listing_map.file_name = Some(
        new_osu_file_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string(),
    );

    listing.beatmaps.push(new_listing_map);
    listing
        .save(osu_path.join("osu!.db"))
        .expect("Failed to write to osu!.db");
    println!("map added to osu!.db");
}
