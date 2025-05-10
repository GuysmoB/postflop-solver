use postflop_solver::card_to_string_simple;
use postflop_solver::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Write};
use std::path::Path;

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
    oop_bet_sizes: String,
    oop_raise_sizes: String,
    ip_bet_sizes: String,
    ip_raise_sizes: String,
    turn_donk_sizes: Option<String>,
    river_donk_sizes: Option<String>,
    add_allin_threshold: f64,
    force_allin_threshold: f64,
    merging_threshold: f64,
    max_iterations: u32,
    target_exploitability: f32,
    use_compression: bool,
}

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

    let oop_bet_sizes = BetSizeOptions::try_from((
        config.oop_bet_sizes.as_str(),
        config.oop_raise_sizes.as_str(),
    ))
    .unwrap();

    let ip_bet_sizes =
        BetSizeOptions::try_from((config.ip_bet_sizes.as_str(), config.ip_raise_sizes.as_str()))
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
        flop_bet_sizes: [oop_bet_sizes.clone(), ip_bet_sizes.clone()],
        turn_bet_sizes: [oop_bet_sizes.clone(), ip_bet_sizes.clone()],
        river_bet_sizes: [oop_bet_sizes, ip_bet_sizes],
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
    let target_exploitability = config.target_exploitability;
    let print_progress = true;
    let mut exploitability = compute_exploitability(&game);

    if print_progress {
        print!("iteration: 0 / {max_iterations} ");
        print!("(exploitability = {exploitability:.4e})");
        use std::io::{self, Write};
        io::stdout().flush().unwrap();
    }

    for current_iteration in 0..max_iterations {
        if exploitability <= target_exploitability {
            break;
        }

        solve_step(&mut game, current_iteration);

        if (current_iteration + 1) % 10 == 0 || current_iteration + 1 == max_iterations {
            exploitability = compute_exploitability(&game);
        }

        if print_progress {
            print!(
                "\riteration: {} / {} ",
                current_iteration + 1,
                max_iterations
            );
            print!("(exploitability = {exploitability:.4e})");
            io::stdout().flush().unwrap();
        }
    }

    if print_progress {
        println!();
    }

    finalize(&mut game);
    println!("Exploitability: {:.2}", exploitability);

    game.back_to_root();
    explore_and_save_ranges(&mut game, "solver_results", 3)?;
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

fn log_game_state(game: &PostFlopGame) {
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
    println!("Rake rate: {}", tree_config.rake_rate);
    println!("Rake cap: {}", tree_config.rake_cap);
    println!("Add allin threshold: {}", tree_config.add_allin_threshold);
    println!(
        "Force allin threshold: {}",
        tree_config.force_allin_threshold
    );
    println!("Merging threshold: {}", tree_config.merging_threshold);

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
