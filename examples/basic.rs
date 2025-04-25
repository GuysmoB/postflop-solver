use postflop_solver::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::json;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;

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

// Ajout de l'implémentation de Deserialize
impl<'de> Deserialize<'de> for Round2 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Désérialiser comme f32 et encapsuler dans Round2
        let value = f32::deserialize(deserializer)?;
        Ok(Round2(value))
    }
}

// Define structures for JSON serialization
#[derive(Serialize, Deserialize)]
struct HandStrategy {
    strategy: Vec<Round2>,
    ev: Vec<Round2>,
}

#[derive(Serialize, Deserialize)]
struct NodeStrategy {
    position: String,
    actions: Vec<String>,
    hands: HashMap<String, HandStrategy>,
    equity: [Round2; 2], // Changé de f32 à Round2
    ev: [Round2; 2],     // Changé de f32 à Round2
}

fn main() {
    // ranges of OOP and IP in string format
    // see the documentation of `Range` for more details about the format
    let oop_range = "66+,A8s+,A5s-A4s,AJo+,K9s+,KQo,QTs+,JTs,96s+,85s+,75s+,65s,54s";
    let ip_range = "QQ-22,AQs-A2s,ATo+,K5s+,KJo+,Q8s+,J8s+,T7s+,96s+,86s+,75s+,64s+,53s+";

    let card_config = CardConfig {
        range: [oop_range.parse().unwrap(), ip_range.parse().unwrap()],
        flop: flop_from_str("Td9d6h").unwrap(),
        turn: card_from_str("Qc").unwrap(),
        river: NOT_DEALT,
    };

    // bet sizes -> 60% of the pot, geometric size, and all-in
    // raise sizes -> 2.5x of the previous bet
    // see the documentation of `BetSizeOptions` for more details
    let bet_sizes = BetSizeOptions::try_from(("60%, e, a", "2.5x")).unwrap();

    let tree_config = TreeConfig {
        initial_state: BoardState::Turn, // must match `card_config`
        starting_pot: 200,
        effective_stack: 900,
        rake_rate: 0.0,
        rake_cap: 0.0,
        flop_bet_sizes: [bet_sizes.clone(), bet_sizes.clone()], // [OOP, IP]
        turn_bet_sizes: [bet_sizes.clone(), bet_sizes.clone()],
        river_bet_sizes: [bet_sizes.clone(), bet_sizes],
        turn_donk_sizes: None, // use default bet sizes
        river_donk_sizes: Some(DonkSizeOptions::try_from("50%").unwrap()),
        add_allin_threshold: 1.5, // add all-in if (maximum bet size) <= 1.5x pot
        force_allin_threshold: 0.15, // force all-in if (SPR after the opponent's call) <= 0.15
        merging_threshold: 0.1,
    };

    // build the game tree
    // `ActionTree` can be edited manually after construction
    let action_tree = ActionTree::new(tree_config.clone()).unwrap();
    let mut game = PostFlopGame::with_config(card_config.clone(), action_tree).unwrap();

    // obtain the private hands
    let oop_cards = game.private_cards(0);
    let oop_cards_str = holes_to_strings(oop_cards).unwrap();

    // check memory usage
    let (mem_usage, mem_usage_compressed) = game.memory_usage();
    println!(
        "Memory usage without compression (32-bit float): {:.2}GB",
        mem_usage as f64 / (1024.0 * 1024.0 * 1024.0)
    );
    println!(
        "Memory usage with compression (16-bit integer): {:.2}GB",
        mem_usage_compressed as f64 / (1024.0 * 1024.0 * 1024.0)
    );

    // allocate memory without compression (use 32-bit float)
    game.allocate_memory(false);

    // solve the game
    let max_num_iterations = 1000;
    let target_exploitability = game.tree_config().starting_pot as f32 * 0.005; // 0.5% of the pot
    let exploitability = solve(&mut game, max_num_iterations, target_exploitability, true);
    println!("Exploitability: {:.2}", exploitability);

    // Create file for JSON output
    let mut json_file = File::create("solver_results.json").unwrap();

    // Start analysis at the root node
    game.back_to_root();

    // Generate JSON tree
    let json_tree = generate_strategy_json(&mut game, Vec::new());

    // Get the pretty JSON string
    let mut json_string = serde_json::to_string_pretty(&json_tree).unwrap();

    // Apply regex replacements to compact arrays
    use regex::Regex;

    // More comprehensive regex to handle nested arrays too
    // This will compact arrays for specific fields regardless of nesting level
    let array_regex =
        Regex::new(r#""(actions|equity|ev|strategy|path)":\s*\[\s*([^\[\]]*?)\s*]"#).unwrap();
    json_string = array_regex
        .replace_all(&json_string, |caps: &regex::Captures| {
            let field = &caps[1];
            // Remove newlines and extra spaces but keep commas and quotes
            let content = caps[2].replace("\n", "").replace("  ", "");
            format!("\"{}\": [{}]", field, content)
        })
        .to_string();

    // Write the modified JSON to file
    write!(json_file, "{}", json_string).unwrap();

    println!("JSON results written to solver_results.json");
}

// Function to recursively explore all nodes in the game tree
// Generate JSON for the current node and recursively for child nodes
fn generate_strategy_json(game: &mut PostFlopGame, path: Vec<Action>) -> serde_json::Value {
    // Skip terminal and chance nodes
    if game.is_terminal_node() || game.is_chance_node() {
        return json!({
            "type": if game.is_terminal_node() { "terminal" } else { "chance" }
        });
    }

    // Cache weights
    game.cache_normalized_weights();

    // Get node information
    let player = game.current_player();
    let position = if player == 0 { "OOP" } else { "IP" };
    let actions = game.available_actions();
    println!("Player: {}, Actions: {:?}", position, actions);

    // Calculate overall equities and EVs
    let equity_oop = Round2(compute_average(&game.equity(0), game.normalized_weights(0)));
    let equity_ip = Round2(compute_average(&game.equity(1), game.normalized_weights(1)));
    let ev_oop = Round2(compute_average(
        &game.expected_values(0),
        game.normalized_weights(0),
    ));
    let ev_ip = Round2(compute_average(
        &game.expected_values(1),
        game.normalized_weights(1),
    ));

    // Get hand information
    let mut hands = HashMap::new();
    let range = game.private_cards(player);
    let range_size = range.len();
    let hand_strings = holes_to_strings(range).unwrap();

    // Get strategy array
    let strategy = game.strategy();

    let target_hand = "Ad2d";
    let mut found = false;

    // For each hand, compute strategy and EVs
    for (h_idx, hand_str) in hand_strings.iter().enumerate() {
        if hand_str != target_hand {
            continue;
        }

        found = true;
        let mut hand_strategy = Vec::new();
        let mut hand_evs = Vec::new();

        // Store current game state
        let current_history = game.history().to_vec();

        // For each action, calculate EV
        for (a_idx, _) in actions.iter().enumerate() {
            // Add strategy frequency for this action
            let strat_index = h_idx + a_idx * range_size;
            let strat_value = if strat_index < strategy.len() {
                Round2(strategy[strat_index])
            } else {
                Round2(0.0) // Default to 0 if index is out of bounds
            };
            hand_strategy.push(strat_value);

            // Calculate EV if we were to take this action
            game.play(a_idx);

            let ev = if !game.is_chance_node() && !game.is_terminal_node() {
                game.cache_normalized_weights();
                let evs = game.expected_values(player);
                if h_idx < evs.len() {
                    Round2(evs[h_idx])
                } else {
                    Round2(0.0) // Default to 0 if index is out of bounds
                }
            } else {
                Round2(0.0) // For chance/terminal nodes, we don't have EV for specific hands
            };

            hand_evs.push(ev);

            // Restore position
            game.back_to_root();
            for &action_idx in current_history.iter() {
                game.play(action_idx);
            }
        }

        hands.insert(
            hand_str.clone(),
            HandStrategy {
                strategy: hand_strategy,
                ev: hand_evs,
            },
        );

        // Sortir de la boucle car on a trouvé la main
        break;
    }

    if !found {
        hands.insert(
            target_hand.to_string(),
            HandStrategy {
                strategy: vec![Round2(0.0); actions.len()],
                ev: vec![Round2(0.0); actions.len()],
            },
        );
    }

    // Créer un NodeStrategy au lieu d'utiliser json! directement
    let node_strategy = NodeStrategy {
        position: position.to_string(),
        actions: actions.iter().map(|a| format!("{:?}", a)).collect(),
        hands: hands,
        equity: [equity_oop, equity_ip],
        ev: [ev_oop, ev_ip],
    };

    // Convertir en serde_json::Value en utilisant to_value
    let mut node_data = serde_json::to_value(&node_strategy).unwrap();

    // Ajouter le chemin séparément car il n'est pas dans NodeStrategy
    let path_json =
        serde_json::to_value(path.iter().map(|a| format!("{:?}", a)).collect::<Vec<_>>()).unwrap();
    node_data
        .as_object_mut()
        .unwrap()
        .insert("path".to_string(), path_json);

    // Store current game state for restoring
    let current_history = game.history().to_vec();

    // Recursively explore child nodes (up to a reasonable depth)
    let mut children = Vec::new();
    if path.len() < 3 {
        // Limit recursion depth
        for (i, action) in actions.iter().enumerate() {
            game.play(i);
            if !game.is_terminal_node() && !game.is_chance_node() {
                let mut new_path = path.clone();
                new_path.push(*action);
                let child = generate_strategy_json(game, new_path);
                children.push(json!({
                    "action": format!("{:?}", action),
                    "data": child
                }));
            }

            // Restore position
            game.back_to_root();
            for &action_idx in current_history.iter() {
                game.play(action_idx);
            }
        }
    }

    // Add children to node data if we have any
    if !children.is_empty() {
        let mut result = node_data.as_object().unwrap().clone();
        result.insert("children".to_string(), json!(children));
        json!(result)
    } else {
        node_data
    }
}
