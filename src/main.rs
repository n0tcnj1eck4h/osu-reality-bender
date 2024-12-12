mod replay_to_beatmap;

use osu_db::collection::Collection;
use osu_db::listing::{Beatmap, Listing};
use osu_db::replay::Replay;
use osu_db::score::{BeatmapScores, ScoreList};
use osu_db::{CollectionList, Mod, ModSet};
use osu_file_parser::OsuFile;
use rayon::prelude::*;
use replay_to_beatmap::convert_replay_to_beatmap;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command()]
struct Args {
    #[command(subcommand)]
    command: SubCommand,
    osu_path: PathBuf,
}

#[derive(Subcommand, Debug)]
enum SubCommand {
    ReplayToBeatmap { replay_path: PathBuf },
    ImportReplaysWIP,
    ImportDatabaseWIP { other_osu_db: PathBuf },
    AddAllMapsToCollection { collection_name: Option<String> },
    DiffCalc { mods: Option<String> },
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let osu_path = args.osu_path;
    println!("Using {} as osu! directory...", osu_path.to_string_lossy());

    match args.command {
        SubCommand::ReplayToBeatmap { replay_path } => {
            println!("Reading osu!.db...");
            let osu_db_path = osu_path.join("osu!.db");
            let mut listing = Listing::from_file(&osu_db_path)
                .expect(format!("Failed to open {}", osu_db_path.to_string_lossy()).as_str());

            let mut replay =
                Replay::from_file(replay_path).expect("Failed to read/parse the replay");

            let listing_map = listing
                .beatmaps
                .iter()
                .find(|e| e.hash == replay.beatmap_hash)
                .expect("Couldn't find the beatmap in osu!.db")
                .clone();

            let beatmap_directory = osu_path.join(
                listing_map
                    .folder_name
                    .as_ref()
                    .expect("Beatmap listing is missing its folder name"),
            );

            let beatmap_file_name = listing_map
                .file_name
                .as_ref()
                .expect("Beatmap listing is missing its file name");

            let beatmap = {
                let beatmap_path = beatmap_directory.join(beatmap_file_name);
                let content = fs::read_to_string(beatmap_path)?;
                content.parse::<OsuFile>()?
            };

            let beatmap = convert_replay_to_beatmap(beatmap, &replay);

            let mut new_osu_file_path = osu_path.join("Songs");
            new_osu_file_path.push(beatmap_directory);
            new_osu_file_path.push(format!("awesome {}", beatmap_file_name));

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
            replay.save("awesomereplay.osr", Some(2))?;

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
            listing.save(osu_path.join("osu!.db"))?;
            println!("map added to osu!.db");
        }
        SubCommand::ImportReplaysWIP => {
            println!("Reading scores.db...");
            let scores_db_path = osu_path.join("scores.db");
            let mut scores = ScoreList::from_file(&scores_db_path)?;

            println!("Sorting scores.db...");
            scores.beatmaps.sort_by(|a, b| a.hash.cmp(&b.hash));

            let mut scores_added = 0;

            let replays_dir = osu_path.join("Data/r");
            'outer: for entry in replays_dir.read_dir()? {
                let path = entry?.path();
                if path.is_file() {
                    if let Some(extension) = path.extension() {
                        if extension == "osr" {
                            let replay = Replay::from_file(path)?;
                            let beatmap_index = scores
                                .beatmaps
                                .binary_search_by(|probe| probe.hash.cmp(&replay.beatmap_hash));

                            match beatmap_index {
                                Ok(index) => {
                                    let map_scores = &mut scores.beatmaps[index].scores;
                                    for map_score in map_scores.iter() {
                                        if map_score.replay_hash == replay.replay_hash {
                                            // println!(
                                            //     "Score {:?} already exists in the database",
                                            //     replay.replay_hash
                                            // );
                                            continue 'outer;
                                        }
                                    }
                                    println!(
                                        "Adding a score with hash {:?} to the database",
                                        replay.replay_hash
                                    );
                                    scores_added += 1;
                                    map_scores.push(replay);
                                }
                                Err(index) => {
                                    println!("Adding a new beatmap with hash {:?} into scores.db with replay hash {:?}", replay.beatmap_hash, replay.replay_hash);
                                    let beatmap_scores = BeatmapScores {
                                        hash: replay.beatmap_hash.clone(),
                                        scores: vec![replay],
                                    };
                                    scores_added += 1;
                                    scores.beatmaps.insert(index, beatmap_scores);
                                }
                            }
                        }
                    }
                }
            }

            if scores_added > 0 {
                let mut scores_db_backup_path = scores_db_path.clone();
                scores_db_backup_path.pop();
                scores_db_backup_path.push("scores.db.backup");

                println!("Backing up scores.db...");
                fs::rename(&scores_db_path, scores_db_backup_path)?;

                println!("Saving scores.db...");
                scores.save(scores_db_path)?;

                println!("Done. {} scores were added.", scores_added);
            } else {
                println!("Nothing changed really. Done.");
            }
        }
        SubCommand::ImportDatabaseWIP { other_osu_db } => {
            let osu_db_path = osu_path.join("osu!.db");

            println!("Reading osu!.db...");
            let mut target_listing = Listing::from_file(osu_db_path)?;

            println!("Reading the source osu!.db...");
            let source_listing = Listing::from_file(other_osu_db)?;

            println!("Sorting maps...");
            target_listing.beatmaps.sort_by(|a, b| a.hash.cmp(&b.hash));

            for new_map in source_listing.beatmaps {
                let index = target_listing
                    .beatmaps
                    .binary_search_by(|probe| probe.hash.cmp(&new_map.hash));

                match index {
                    Ok(_existing_map_index) => {
                        println!(
                            "Merging difficulty ratings for beatmap {}",
                            new_map.beatmap_id
                        );
                        // target_listing.beatmaps[existing_map_index].std_ratings;
                    }
                    Err(insert_index) => {
                        println!("Added new map {} to the database", new_map.beatmap_id);
                        target_listing.beatmaps.insert(insert_index, new_map);
                    }
                }
            }
        }
        SubCommand::AddAllMapsToCollection { collection_name } => {
            println!("Reading osu!.db...");
            let osu_db_path = osu_path.join("osu!.db");
            let listing = Listing::from_file(osu_db_path)?;

            println!("Reading collection.db...");
            let collection_path = osu_path.join("collection.db");
            let mut collection = CollectionList::from_file(&collection_path)?;

            let collection_name = collection_name.unwrap_or("All maps".to_string());

            let mut all_maps_collection = Collection {
                name: Some(collection_name.clone()),
                beatmap_hashes: Vec::new(),
            };

            for map in listing.beatmaps {
                if map.hash.is_some() {
                    all_maps_collection.beatmap_hashes.push(map.hash)
                }
            }

            println!(
                "Added {} maps to a new collection \"{}\"",
                all_maps_collection.beatmap_hashes.len(),
                collection_name
            );
            collection.collections.push(all_maps_collection);
            collection.to_file(collection_path)?;
        }
        SubCommand::DiffCalc { .. } => {
            let osu_db_path = osu_path.join("osu!.db");

            println!("Reading osu!.db...");
            let mut listing = Listing::from_file(&osu_db_path)?;

            let osu_cfg_filename = format!("osu!.{}.cfg", whoami::username());
            let cfg_path = osu_path.join(osu_cfg_filename);

            let mut songs_path = "Songs".to_string();

            for line in fs::read_to_string(cfg_path)?.lines() {
                if line.starts_with("BeatmapDirectory") {
                    if let Some(val) = line.split("=").nth(1) {
                        songs_path = val.trim().to_string();
                        break;
                    }
                }
            }

            let songs_path = osu_path.join(&songs_path);
            println!("Songs path is {}", songs_path.display());

            let calculated_maps = AtomicUsize::new(0);
            let skipped_maps = AtomicU32::new(0);
            let print_thing = AtomicU32::new(0);
            let map_count = listing.beatmaps.len();

            let nomod = ModSet(0);
            let hr = ModSet(1 << Mod::HardRock.raw() as u32);
            let dt = ModSet(1 << Mod::DoubleTime.raw() as u32);
            let dthr = dt.with(Mod::HardRock);
            let wanted_modsets = &[nomod, hr, dt, dthr];

            let beatmaps: Vec<Beatmap> = listing
                .beatmaps
                .into_par_iter()
                .map(|mut beatmap| {
                    let missing_map_modset_calcs: Vec<_> = wanted_modsets
                        .iter()
                        .filter(|modset| {
                            beatmap
                                .std_ratings
                                .iter()
                                .find(|e| e.0 == **modset)
                                .is_none()
                        })
                        .collect();

                    if missing_map_modset_calcs.is_empty() {
                        skipped_maps.fetch_add(1, Ordering::Relaxed);
                        return beatmap;
                    }

                    let mut map_path = songs_path.join(beatmap.folder_name.as_ref().unwrap());
                    map_path.push(beatmap.file_name.as_ref().unwrap());

                    if let Ok(map) = rosu_pp::Beatmap::from_path(map_path) {
                        for modset in missing_map_modset_calcs {
                            let diff_attrs =
                                rosu_pp::Difficulty::new().mods(modset.0).calculate(&map);
                            let stars = diff_attrs.stars();
                            beatmap.std_ratings.push((*modset, stars));
                        }

                        calculated_maps.fetch_add(1, Ordering::Relaxed);
                        print_thing.fetch_add(1, Ordering::Relaxed);

                        if print_thing.load(Ordering::Relaxed) == 128 {
                            let calculated_maps = calculated_maps.load(Ordering::Relaxed);
                            let skipped_maps = skipped_maps.load(Ordering::Relaxed);
                            print!(
                                "\rCalculated {}/{} maps. {} skipped.",
                                calculated_maps, map_count, skipped_maps
                            );
                            std::io::stdout()
                                .flush()
                                .ok()
                                .expect("Could not flush stdout");
                            print_thing.store(0, Ordering::Relaxed);
                        }
                    }

                    beatmap
                })
                .collect();

            listing.beatmaps = beatmaps;

            let calculated_maps = calculated_maps.load(Ordering::Relaxed);
            println!(
                "\rCalcualted {} maps. Skipped {}. {} maps were already calculated.",
                calculated_maps,
                skipped_maps.load(Ordering::Relaxed),
                map_count - calculated_maps,
            );

            println!("Saving database...");
            listing.save(osu_db_path)?;
            println!("Done.");
        }
    }

    Ok(())
}
