use postflop_solver::card_to_string_simple;
use postflop_solver::*;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, Debug)]
struct SolverConfig {
    oop_range: String,
    ip_range: String,
    flop: String,
    turn: Option<String>,
    river: Option<String>,
    initial_state: String,
    starting_pot: i32,
    effective_stack: i32,
    rake_rate: f64,
    rake_cap: f64,
    flop_oop_bet_sizes: String,
    flop_oop_raise_sizes: String,
    flop_ip_bet_sizes: String,
    flop_ip_raise_sizes: String,
    turn_oop_bet_sizes: String,
    turn_oop_raise_sizes: String,
    turn_ip_bet_sizes: String,
    turn_ip_raise_sizes: String,
    river_oop_bet_sizes: String,
    river_oop_raise_sizes: String,
    river_ip_bet_sizes: String,
    river_ip_raise_sizes: String,
    turn_donk_sizes: Option<String>,
    river_donk_sizes: Option<String>,
    add_allin_threshold: f64,
    force_allin_threshold: f64,
    merging_threshold: f64,
    max_iterations: u32,
    target_exploitability: f32,
    use_compression: bool,
    max_runtime_seconds: Option<u64>,
    saved_folder: Option<String>,
}

//cargo run --release --example run-config -- examples/config_file.json
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        println!("Usage: {} <config_file.json>", args[0]);
        return Ok(());
    }

    let config = load_config(&args[1])?;
    let flop = flop_from_str(&config.flop).unwrap();
    let turn = if let Some(turn_str) = &config.turn {
        card_from_str(turn_str).unwrap()
    } else {
        NOT_DEALT
    };
    let river = if let Some(river_str) = &config.river {
        card_from_str(river_str).unwrap()
    } else {
        NOT_DEALT
    };

    let card_config = CardConfig {
        range: [
            config.oop_range.parse().unwrap(),
            config.ip_range.parse().unwrap(),
        ],
        flop,
        turn,
        river,
    };

    let flop_oop_bet_sizes = BetSizeOptions::try_from((
        config.flop_oop_bet_sizes.as_str(),
        config.flop_oop_raise_sizes.as_str(),
    ))
    .unwrap();

    let flop_ip_bet_sizes = BetSizeOptions::try_from((
        config.flop_ip_bet_sizes.as_str(),
        config.flop_ip_raise_sizes.as_str(),
    ))
    .unwrap();

    let turn_oop_bet_sizes = BetSizeOptions::try_from((
        config.turn_oop_bet_sizes.as_str(),
        config.turn_oop_raise_sizes.as_str(),
    ))
    .unwrap();

    let turn_ip_bet_sizes = BetSizeOptions::try_from((
        config.turn_ip_bet_sizes.as_str(),
        config.turn_ip_raise_sizes.as_str(),
    ))
    .unwrap();

    let river_oop_bet_sizes = BetSizeOptions::try_from((
        config.river_oop_bet_sizes.as_str(),
        config.river_oop_raise_sizes.as_str(),
    ))
    .unwrap();

    let river_ip_bet_sizes = BetSizeOptions::try_from((
        config.river_ip_bet_sizes.as_str(),
        config.river_ip_raise_sizes.as_str(),
    ))
    .unwrap();

    let turn_donk_sizes = config
        .turn_donk_sizes
        .as_ref()
        .map(|s| DonkSizeOptions::try_from(s.as_str()).unwrap());

    let river_donk_sizes = config
        .river_donk_sizes
        .as_ref()
        .map(|s| DonkSizeOptions::try_from(s.as_str()).unwrap());

    let initial_state = match config.initial_state.to_lowercase().as_str() {
        "flop" => BoardState::Flop,
        "turn" => BoardState::Turn,
        "river" => BoardState::River,
        _ => BoardState::Flop,
    };

    let tree_config = TreeConfig {
        initial_state,
        starting_pot: config.starting_pot,
        effective_stack: config.effective_stack,
        rake_rate: config.rake_rate,
        rake_cap: config.rake_cap,
        flop_bet_sizes: [flop_oop_bet_sizes, flop_ip_bet_sizes],
        turn_bet_sizes: [turn_oop_bet_sizes, turn_ip_bet_sizes],
        river_bet_sizes: [river_oop_bet_sizes, river_ip_bet_sizes],
        turn_donk_sizes,
        river_donk_sizes,
        add_allin_threshold: config.add_allin_threshold,
        force_allin_threshold: config.force_allin_threshold,
        merging_threshold: config.merging_threshold,
    };

    let action_tree = ActionTree::new(tree_config.clone())?;
    let mut game = PostFlopGame::with_config(card_config, action_tree)?;
    game.allocate_memory(config.use_compression);

    let max_iterations = config.max_iterations;
    let target_exploitability = config.target_exploitability * 100.0;
    let mut exploitability = compute_exploitability(&game);
    let start_time = Instant::now();
    let timeout = config
        .max_runtime_seconds
        .map(|secs| Duration::from_secs(secs));

    // log_game_state(&game, &config);

    for current_iteration in 0..max_iterations {
        if let Some(max_duration) = timeout {
            if start_time.elapsed() >= max_duration {
                println!(
                    "Solver stopped due to time limit ({} seconds), iteration: {}, exploitability: {:.2}%",
                    max_duration.as_secs(), current_iteration, exploitability
                );
                break;
            }
        }

        if exploitability <= target_exploitability {
            println!(
                "Solver stopped due to target exploitability reached ({:.2}%), iteration: {}, time: {:.2} seconds",
                exploitability, current_iteration, start_time.elapsed().as_secs_f64()
            );
            break;
        }

        solve_step(&mut game, current_iteration);

        if (current_iteration + 1) % 10 == 0 || current_iteration + 1 == max_iterations {
            exploitability = compute_exploitability(&game);
        }

        if current_iteration + 1 == max_iterations {
            println!(
                "Solver stopped due to max iterations reached: {}, exploitability: {:.2}%, time: {:.2} seconds",
                max_iterations, exploitability, start_time.elapsed().as_secs_f64()
            );
        }
    }

    // println!(
    //     "Solver finished after {:.2} seconds (exploitability: {:.2}%)",
    //     start_time.elapsed().as_secs_f64(),
    //     exploitability
    // );

    finalize(&mut game);
    game.back_to_root();

    let output_folder = match &config.saved_folder {
        Some(folder) => format!("results/{}", folder),
        None => "results".to_string(),
    };

    explore_and_save_ranges(&mut game, output_folder.as_str(), 10)?;
    // run_bet_call_turn_scenario(&mut game)?;
    // explore_game_tree(&mut game);
    Ok(())
}

fn load_config(filename: &str) -> Result<SolverConfig, Box<dyn std::error::Error>> {
    let path = Path::new(filename);
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    // Utiliser serde_json pour charger le fichier JSON
    let config: SolverConfig = serde_json::from_reader(reader)?;

    println!("Configuration chargée depuis: {}", filename);
    Ok(config)
}

fn log_game_state(game: &PostFlopGame, config: &SolverConfig) {
    println!("\n===== POSTFLOP GAME INITIAL STATE =====");

    // Card configuration
    let card_config = game.card_config();
    println!("\n--- Card Configuration ---");

    // Board state
    let flop_str = card_config
        .flop
        .iter()
        .map(|&c| card_to_string_simple(c))
        .collect::<Vec<_>>()
        .join(" ");
    println!("Flop: {}", flop_str);

    // Turn if exists
    if card_config.turn != NOT_DEALT {
        println!("Turn: {}", card_to_string_simple(card_config.turn));
    } else {
        println!("Turn: Not dealt");
    }

    // River if exists
    if card_config.river != NOT_DEALT {
        println!("River: {}", card_to_string_simple(card_config.river));
    } else {
        println!("River: Not dealt");
    }

    // Tree configuration
    let tree_config = game.tree_config();
    println!("\n--- Tree Configuration ---");
    println!("Initial state: {:?}", tree_config.initial_state);
    println!("Starting pot: {}", tree_config.starting_pot);
    println!("Effective stack: {}", tree_config.effective_stack);
    println!("Rake rate: {}", tree_config.rake_rate * 100.0);
    println!("Rake cap: {}", tree_config.rake_cap);
    println!(
        "Add allin threshold: {}%",
        tree_config.add_allin_threshold * 100.0
    );
    println!(
        "Force allin threshold: {}%",
        tree_config.force_allin_threshold * 100.0
    );
    println!(
        "Merging threshold: {}%",
        tree_config.merging_threshold * 100.0
    );
    println!(
        "Target exploitability: {}%",
        config.target_exploitability * 100.0
    );
    println!("Max iterations: {}", config.max_iterations);

    // Bet size information
    println!("\n--- Bet Size Configuration ---");
    println!("FLOP:");
    println!("  OOP bet: {:?}", tree_config.flop_bet_sizes[0].bet);
    println!("  OOP raise: {:?}", tree_config.flop_bet_sizes[0].raise);
    println!("  IP bet: {:?}", tree_config.flop_bet_sizes[1].bet);
    println!("  IP raise: {:?}", tree_config.flop_bet_sizes[1].raise);

    println!("TURN:");
    println!("  OOP bet: {:?}", tree_config.turn_bet_sizes[0].bet);
    println!("  OOP raise: {:?}", tree_config.turn_bet_sizes[0].raise);
    println!("  IP bet: {:?}", tree_config.turn_bet_sizes[1].bet);
    println!("  IP raise: {:?}", tree_config.turn_bet_sizes[1].raise);
    if let Some(donk) = &tree_config.turn_donk_sizes {
        println!("  OOP donk: {:?}", donk.donk);
    } else {
        println!("  OOP donk: None");
    }

    println!("RIVER:");
    println!("  OOP bet: {:?}", tree_config.river_bet_sizes[0].bet);
    println!("  OOP raise: {:?}", tree_config.river_bet_sizes[0].raise);
    println!("  IP bet: {:?}", tree_config.river_bet_sizes[1].bet);
    println!("  IP raise: {:?}", tree_config.river_bet_sizes[1].raise);
    if let Some(donk) = &tree_config.river_donk_sizes {
        println!("  OOP donk: {:?}", donk.donk);
    } else {
        println!("  OOP donk: None");
    }

    // Range information
    println!("\n--- Range Information ---");
    println!("OOP hands: {} combos", game.private_cards(0).len());
    println!("IP hands: {} combos", game.private_cards(1).len());

    println!("\n====================================");
}
