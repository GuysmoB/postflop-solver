use flate2::write::GzEncoder;
use flate2::Compression;
use postflop_solver::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Read, Write};

// Utilisez les fonctions avec la notation du module
use postflop_solver::card_to_string_simple;

// Custom serializer for f32 values that rounds to 2 decimal places
#[derive(Clone, Copy)]
struct Round2(f32);

impl Serialize for Round2 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Round to 2 decimal places
        let rounded = (self.0 * 100.0).round() / 100.0;
        serializer.serialize_f32(rounded)
    }
}

impl<'de> Deserialize<'de> for Round2 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = f32::deserialize(deserializer)?;
        Ok(Round2(value))
    }
}

// Structure for the strategy output
#[derive(Serialize, Deserialize)]
struct StrategyOutput {
    actions: Vec<String>,
    #[serde(rename = "childrens")]
    children: HashMap<String, NodeData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    node_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    player: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    strategy: Option<StrategyData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deal_number: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dealcards: Option<HashMap<String, NodeData>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize)]
struct StrategyData {
    actions: Vec<String>,
    strategy: HashMap<String, Vec<Round2>>,
}

// Node data can either be a full node or a reference node
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum NodeData {
    FullNode(StrategyOutput),
    Reference {
        deal_number: usize,
        node_type: String,
    },
}

fn main() {
    // ranges of OOP and IP in string format
    let oop_range = "66+,A8s+,A5s-A4s,AJo+,K9s+,KQo,QTs+,JTs,96s+,85s+,75s+,65s,54s";
    let ip_range = "QQ-22,AQs-A2s,ATo+,K5s+,KJo+,Q8s+,J8s+,T7s+,96s+,86s+,75s+,64s+,53s+";

    let card_config = CardConfig {
        range: [oop_range.parse().unwrap(), ip_range.parse().unwrap()],
        flop: flop_from_str("9d5s3d").unwrap(),
        turn: NOT_DEALT, // card_from_str("Qc").unwrap(),
        river: NOT_DEALT,
    };

    let bet_sizes = BetSizeOptions::try_from(("50%, 100%, a", "2x, a")).unwrap();

    let tree_config = TreeConfig {
        initial_state: BoardState::Flop,
        starting_pot: 20,
        effective_stack: 200,
        rake_rate: 0.0,
        rake_cap: 0.0,
        flop_bet_sizes: [bet_sizes.clone(), bet_sizes.clone()],
        turn_bet_sizes: [bet_sizes.clone(), bet_sizes.clone()],
        river_bet_sizes: [bet_sizes.clone(), bet_sizes],
        turn_donk_sizes: Some(DonkSizeOptions::try_from("50%").unwrap()),
        river_donk_sizes: Some(DonkSizeOptions::try_from("50%").unwrap()),
        add_allin_threshold: 1.5,
        force_allin_threshold: 0.20,
        merging_threshold: 0.1,
    };

    // Construction et résolution du jeu
    let action_tree = ActionTree::new(tree_config.clone()).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    // log_game_state(&game);

    // Allocation de mémoire
    game.allocate_memory(false);

    // Paramètres de résolution
    let max_iterations = 10;
    let target_exploitability = 0.03;
    let print_progress = true;

    println!("Démarrage de la résolution avec solve_step et finalize...");

    // Version manuelle de solve() avec solve_step
    let mut exploitability = compute_exploitability(&game);

    // Afficher l'exploitabilité initiale
    if print_progress {
        print!("iteration: 0 / {max_iterations} ");
        print!("(exploitability = {exploitability:.4e})");
        use std::io::{self, Write};
        io::stdout().flush().unwrap();
    }

    // Boucle principale de résolution
    for current_iteration in 0..max_iterations {
        // Vérifier si l'exploitabilité cible est atteinte
        if exploitability <= target_exploitability {
            break;
        }

        // Exécuter une itération du solver
        solve_step(&mut game, current_iteration);

        // Calculer l'exploitabilité toutes les 10 itérations ou à la fin
        if (current_iteration + 1) % 10 == 0 || current_iteration + 1 == max_iterations {
            exploitability = compute_exploitability(&game);
        }

        // Afficher la progression
        if print_progress {
            print!(
                "\riteration: {} / {} ",
                current_iteration + 1,
                max_iterations
            );
            print!("(exploitability = {exploitability:.4e})");
            // io::stdout().flush().unwrap();
        }
    }

    if print_progress {
        println!();
    }

    // Finaliser la solution
    finalize(&mut game);
    save_game_to_file(&game, "game.bin").unwrap();

    println!("Exploitability: {:.2}", exploitability);

    println!("\n=== RÉSULTATS DU PREMIER NŒUD ===");

    // S'assurer que nous sommes à la racine
    game.back_to_root();

    explore_all_paths(&mut game);

    // println!("\n=== DÉTAILS DES MAINS ===");
    // explore_game_tree(&mut game);

    // match run_bet_call_turn_scenario(&mut game) {
    //     Ok(_) => println!("Scénario exécuté avec succès!"),
    //     Err(e) => println!("Erreur: {}", e),
    // }
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

fn save_game_to_file(game: &PostFlopGame, filename: &str) -> Result<(), String> {
    let file = File::create(filename)
        .map_err(|e| format!("Erreur lors de la création du fichier: {}", e))?;

    // Créer un encodeur gzip avec un niveau de compression élevé
    let encoder = GzEncoder::new(file, Compression::best());
    let mut writer = BufWriter::new(encoder);

    // Sérialiser le jeu dans le flux compressé
    bincode::encode_into_std_write(game, &mut writer, bincode::config::standard())
        .map_err(|e| format!("Erreur lors de la sérialisation: {}", e))?;

    // Finaliser l'écriture
    writer
        .flush()
        .map_err(|e| format!("Erreur lors de la finalisation: {}", e))?;

    Ok(())
}
