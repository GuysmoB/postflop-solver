use postflop_solver::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};

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
        flop: flop_from_str("Td5d3h").unwrap(),
        turn: NOT_DEALT, // card_from_str("Qc").unwrap(),
        river: NOT_DEALT,
    };

    let bet_sizes = BetSizeOptions::try_from(("50%", "60%")).unwrap();

    let tree_config = TreeConfig {
        initial_state: BoardState::Flop,
        starting_pot: 20,
        effective_stack: 100,
        rake_rate: 0.0,
        rake_cap: 0.0,
        flop_bet_sizes: [bet_sizes.clone(), bet_sizes.clone()],
        turn_bet_sizes: [bet_sizes.clone(), bet_sizes.clone()],
        river_bet_sizes: [bet_sizes.clone(), bet_sizes],
        turn_donk_sizes: None,
        river_donk_sizes: Some(DonkSizeOptions::try_from("50%").unwrap()),
        add_allin_threshold: 1.5,
        force_allin_threshold: 0.20,
        merging_threshold: 0.1,
    };

    // Construction et résolution du jeu
    let action_tree = ActionTree::new(tree_config.clone()).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    // Allocation de mémoire
    game.allocate_memory(false);

    // Paramètres de résolution
    let max_iterations = 1000;
    let target_exploitability = 1.0;
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

    println!("Exploitability: {:.2}", exploitability);

    println!("\n=== RÉSULTATS DU PREMIER NŒUD ===");

    // S'assurer que nous sommes à la racine
    game.back_to_root();

    // Récupérer les données brutes
    // let result_buffer = get_results(&mut game);

    // // Parser les données brutes du buffer, comme dans ResultNav.vue
    // let mut offset = 0;

    // // Récupérer les en-têtes (pot sizes et empty flag)
    // let pot_oop = result_buffer[offset];
    // offset += 1;
    // let pot_ip = result_buffer[offset];
    // offset += 1;
    // let is_empty_flag = result_buffer[offset] as usize;
    // offset += 1;

    // println!("Pot OOP: {:.2} bb", pot_oop);
    // println!("Pot IP: {:.2} bb", pot_ip);
    // println!("Empty Flag: {}", is_empty_flag);

    // // Récupérer la taille des ranges
    // let oop_range_size = game.private_cards(0).len();
    // let ip_range_size = game.private_cards(1).len();

    // // Afficher les premières valeurs de poids
    // println!("\n--- Premiers poids ---");
    // println!("OOP: {:.6}", result_buffer[offset]);
    // offset += oop_range_size;
    // println!("IP: {:.6}", result_buffer[offset]);
    // offset += ip_range_size;

    // // Si les ranges ne sont pas vides
    // if is_empty_flag == 0 {
    //     // Récupérer les poids normalisés
    //     println!("\n--- Premiers poids normalisés ---");
    //     println!("OOP: {:.6}", result_buffer[offset]);
    //     offset += oop_range_size;
    //     println!("IP: {:.6}", result_buffer[offset]);
    //     offset += ip_range_size;

    //     // Récupérer les premières valeurs d'équité
    //     println!("\n--- Premières équités ---");
    //     println!("OOP: {:.2}%", result_buffer[offset] * 100.0);
    //     offset += oop_range_size;
    //     println!("IP: {:.2}%", result_buffer[offset] * 100.0);
    //     offset += ip_range_size;

    //     // Récupérer les premières valeurs d'EV
    //     println!("\n--- Premières EV ---");
    //     println!("OOP: {:.2} bb", result_buffer[offset]);
    //     offset += oop_range_size;
    //     println!("IP: {:.2} bb", result_buffer[offset]);
    //     offset += ip_range_size;

    //     // Récupérer les premiers ratios EQR
    //     println!("\n--- Premiers ratios EQR ---");
    //     println!("OOP: {:.4}", result_buffer[offset]);
    //     offset += oop_range_size;
    //     println!("IP: {:.4}", result_buffer[offset]);
    //     offset += ip_range_size;
    // }

    // // Récupérer les stratégies et EVs par action
    // if !game.is_terminal_node() && !game.is_chance_node() {
    //     let player = game.current_player();
    //     let actions = game.available_actions();
    //     let range_size = game.private_cards(player).len();

    //     println!("\n--- Stratégies par action (première main) ---");
    //     for (i, action) in actions.iter().enumerate() {
    //         let action_str = format!("{:?}", action)
    //             .to_uppercase()
    //             .replace("(", " ")
    //             .replace(")", "");

    //         let strat_value = result_buffer[offset + i * range_size];
    //         println!("{}: {:.2}%", action_str, strat_value * 100.0);
    //     }

    //     // Si nous avons des EV détaillées par action
    //     if is_empty_flag == 0 {
    //         offset += actions.len() * range_size;
    //         println!("\n--- EV par action (première main) ---");
    //         for (i, action) in actions.iter().enumerate() {
    //             let action_str = format!("{:?}", action)
    //                 .to_uppercase()
    //                 .replace("(", " ")
    //                 .replace(")", "");

    //             let ev_value = result_buffer[offset + i * range_size];
    //             println!("{}: {:.2} bb", action_str, ev_value);
    //         }
    //     }
    // }

    // // Ajouter aussi des statistiques structurées avec get_node_statistics
    // println!("\n=== STATISTIQUES STRUCTURÉES DU NŒUD ===");
    // let stats = get_node_statistics(&mut game);

    // // Afficher les EV par action si disponibles
    // if let Some(action_evs) = stats.get("action_evs") {
    //     if let serde_json::Value::Array(evs) = action_evs {
    //         println!("EV moyenne par action:");
    //         for ev_pair in evs {
    //             if let serde_json::Value::Array(pair) = ev_pair {
    //                 if pair.len() >= 2 {
    //                     println!("  {} : {} bb", pair[0], pair[1]);
    //                 }
    //             }
    //         }
    //     }
    // }

    println!("\n=== DÉTAILS DES MAINS ===");
    // print_hand_details(&mut game, 5);
    explore_random_path(&mut game);
    // println!("\nExploration de tous les chemins d'actions possibles:");
    // println!("\n=== STATISTIQUES DU NŒUD ACTUEL ===");
    // let stats = get_node_statistics(&mut game);
    // let json_string = serde_json::to_string_pretty(&stats).unwrap();
    // println!("{}", json_string);
}
