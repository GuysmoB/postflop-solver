use postflop_solver::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};

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
    // Existing code for setting up ranges, configs, etc.
    let oop_range = "66+,A8s+,A5s-A4s,AJo+,K9s+,KQo,QTs+,JTs,96s+,85s+,75s+,65s,54s";
    let ip_range = "QQ-22,AQs-A2s,ATo+,K5s+,KJo+,Q8s+,J8s+,T7s+,96s+,86s+,75s+,64s+,53s+";

    let card_config = CardConfig {
        range: [oop_range.parse().unwrap(), ip_range.parse().unwrap()],
        flop: flop_from_str("Td9d6h").unwrap(),
        turn: card_from_str("Qc").unwrap(),
        river: NOT_DEALT,
    };

    let bet_sizes = BetSizeOptions::try_from(("60%, e, a", "2.5x")).unwrap();

    let tree_config = TreeConfig {
        initial_state: BoardState::Turn,
        starting_pot: 200,
        effective_stack: 900,
        rake_rate: 0.0,
        rake_cap: 0.0,
        flop_bet_sizes: [bet_sizes.clone(), bet_sizes.clone()],
        turn_bet_sizes: [bet_sizes.clone(), bet_sizes.clone()],
        river_bet_sizes: [bet_sizes.clone(), bet_sizes],
        turn_donk_sizes: None,
        river_donk_sizes: Some(DonkSizeOptions::try_from("50%").unwrap()),
        add_allin_threshold: 1.5,
        force_allin_threshold: 0.15,
        merging_threshold: 0.1,
    };

    let action_tree = ActionTree::new(tree_config.clone()).unwrap();
    let mut game = PostFlopGame::with_config(card_config.clone(), action_tree).unwrap();

    println!("Solving game...");
    let max_num_iterations = 1000;
    let target_exploitability = game.tree_config().starting_pot as f32 * 0.005;
    game.allocate_memory(false);
    let exploitability = solve(&mut game, max_num_iterations, target_exploitability, true);
    println!("Exploitability: {:.2}", exploitability);

    // Create file for JSON output
    let mut json_file = File::create("solver_results.json").unwrap();

    // Start analysis at the root node and generate strategy
    game.back_to_root();
    let strategy_output = generate_strategy(&mut game, Vec::new());

    // Serialize with compact arrays
    let mut json_string = serde_json::to_string_pretty(&strategy_output).unwrap();

    use regex::Regex;
    let array_regex = Regex::new(r#"(\[\s*)([^\[\]]*?)(\s*\])"#).unwrap();

    json_string = array_regex
        .replace_all(&json_string, |caps: &regex::Captures| {
            let start = &caps[1];
            let content_replaced = caps[2].replace("\n", "").replace("  ", "");
            let content = content_replaced.trim();
            let end = &caps[3];
            format!("{}{}{}", start, content, end)
        })
        .to_string();

    // Write the modified JSON to file
    write!(json_file, "{}", json_string).unwrap();
    println!("JSON results written to solver_results.json");

    let json_path = "solver_results.json";
    let hand = "As2s";
    let action_path = &["CHECK CHECK"];
    let expected_player = "IP";

    match find_hand_strategy_in_json(json_path, hand, action_path, Some(expected_player)) {
        Ok((position, actions, frequencies)) => {
            println!(
                "Stratégie pour {} (position: {}) au chemin {:?}:",
                hand, position, action_path
            );
            for (i, action) in actions.iter().enumerate() {
                println!("  {} : {:.2}%", action, frequencies[i] * 100.0);
            }
        }
        Err(e) => println!("Erreur: {}", e),
    }

    println!("\nAffichage de tous les chemins possibles:");
    if let Err(e) = print_all_strategy_paths("solver_results.json") {
        println!("Erreur: {}", e);
    }
}

fn generate_strategy(game: &mut PostFlopGame, path: Vec<String>) -> StrategyOutput {
    if game.is_terminal_node() {
        return StrategyOutput {
            actions: vec![],
            children: HashMap::new(),
            node_type: Some("terminal_node".to_string()),
            player: None,
            strategy: None,
            deal_number: None,
            dealcards: None,
            path: Some(path),
        };
    }

    if game.is_chance_node() {
        let mut dealcards = HashMap::new();

        // Si c'est un nœud de chance, nous devons fournir des informations pour chaque carte possible
        // Ici, on crée simplement une référence
        dealcards.insert(
            "card_placeholder".to_string(),
            NodeData::Reference {
                deal_number: 0,
                node_type: "chance_node".to_string(),
            },
        );

        return StrategyOutput {
            actions: vec![],
            children: HashMap::new(),
            node_type: Some("chance_node".to_string()),
            player: None,
            strategy: None,
            deal_number: Some(52), // Nombre total de cartes dans le deck
            dealcards: Some(dealcards),
            path: Some(path),
        };
    }

    // Regular action node
    game.cache_normalized_weights();
    let player = game.current_player();
    let player_str = if player == 0 { "OOP" } else { "IP" };
    let actions = game.available_actions();
    let action_strings: Vec<String> = actions
        .iter()
        .map(|a| {
            format!("{:?}", a)
                .to_uppercase()
                .replace("(", " ")
                .replace(")", "")
        })
        .collect();

    // Get ranges and strategies
    let range = game.private_cards(player);
    let range_size = range.len();
    let hand_strings = holes_to_strings(range).unwrap();
    let strategy_array = game.strategy();

    // Build strategy data
    let mut strategy_map = HashMap::new();

    let target_hand = "As2s";
    let h_idx = hand_strings.iter().position(|h| h == target_hand);

    // Analyze each hand
    if let Some(h_idx) = h_idx {
        let mut hand_strategy = Vec::new();

        for a_idx in 0..actions.len() {
            let strat_index = h_idx + a_idx * range_size;
            let strat_value = if strat_index < strategy_array.len() {
                Round2(strategy_array[strat_index])
            } else {
                Round2(0.0)
            };
            hand_strategy.push(strat_value);
        }

        // Ajouter uniquement la main cible
        strategy_map.insert(target_hand.to_string(), hand_strategy);
    }

    // Create strategy data object
    let strategy_data = StrategyData {
        actions: action_strings.clone(),
        strategy: strategy_map,
    };

    // Build children nodes
    let mut children = HashMap::new();
    let current_history = game.history().to_vec();

    for (i, action) in actions.iter().enumerate() {
        game.play(i);

        let action_str = format!("{:?}", action)
            .to_uppercase()
            .replace("(", " ")
            .replace(")", "");

        // Créer un nouveau chemin pour ce nœud enfant
        let mut new_path = path.clone();
        new_path.push(action_str.clone());

        let child_node = if game.is_terminal_node() {
            NodeData::Reference {
                deal_number: 0,
                node_type: "terminal_node".to_string(),
            }
        } else if game.is_chance_node() {
            NodeData::Reference {
                deal_number: 0,
                node_type: "chance_node".to_string(),
            }
        } else {
            // Passer le nouveau chemin au nœud enfant
            NodeData::FullNode(generate_strategy(game, new_path))
        };

        children.insert(action_str, child_node);

        // Restore position
        game.back_to_root();
        for &action_idx in current_history.iter() {
            game.play(action_idx);
        }
    }

    StrategyOutput {
        actions: action_strings,
        children,
        node_type: Some("action_node".to_string()),
        player: Some(player_str.to_string()), // Utiliser la chaîne au lieu du nombre
        strategy: Some(strategy_data),
        deal_number: None,
        dealcards: None,
        path: Some(path),
    }
}

fn find_hand_strategy_in_json(
    json_path: &str,
    hand: &str,
    action_path: &[&str],
    expected_position: Option<&str>, // Nouveau paramètre optionnel
) -> Result<(String, Vec<String>, Vec<f32>), String> {
    // Lire le fichier JSON
    let mut file = File::open(json_path)
        .map_err(|e| format!("Erreur lors de l'ouverture du fichier: {}", e))?;

    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| format!("Erreur lors de la lecture du fichier: {}", e))?;

    // Parser le JSON
    let mut json: Value = serde_json::from_str(&content)
        .map_err(|e| format!("Erreur lors du parsing JSON: {}", e))?;

    // Naviguer à travers le chemin d'action
    for action in action_path {
        // Vérifier si nous avons des enfants
        let children = json
            .get("childrens")
            .ok_or_else(|| format!("Pas d'enfants au nœud actuel"))?;

        // Chercher l'action spécifique
        json = children
            .get(action)
            .ok_or_else(|| format!("Action '{}' introuvable", action))?
            .clone();
    }

    // Récupérer la position du joueur au nœud actuel
    let position = json
        .get("player")
        .ok_or_else(|| format!("Information du joueur non disponible"))?
        .as_str()
        .ok_or_else(|| format!("Position du joueur n'est pas une chaîne"))?
        .to_string();

    // Vérifier si la position correspond à celle attendue
    if let Some(exp_pos) = expected_position {
        if position != exp_pos {
            return Err(format!(
                "Position du joueur incorrecte: attendu {}, trouvé {}",
                exp_pos, position
            ));
        }
    }

    // Maintenant nous sommes au bon nœud, récupérer la stratégie
    let strategy_node = json
        .get("strategy")
        .ok_or_else(|| format!("Pas de stratégie au nœud actuel"))?;

    // Récupérer les actions disponibles
    let actions = strategy_node
        .get("actions")
        .ok_or_else(|| format!("Pas d'actions dans la stratégie"))?
        .as_array()
        .ok_or_else(|| "Actions n'est pas un tableau".to_string())?;

    let actions: Vec<String> = actions
        .iter()
        .map(|a| a.as_str().unwrap_or("").to_string())
        .collect();

    // Récupérer la stratégie pour la main spécifique
    let hand_strategies = strategy_node
        .get("strategy")
        .ok_or_else(|| format!("Pas de stratégies dans le nœud"))?;

    let hand_strategy = hand_strategies
        .get(hand)
        .ok_or_else(|| format!("Main '{}' introuvable", hand))?
        .as_array()
        .ok_or_else(|| format!("Stratégie pour '{}' n'est pas un tableau", hand))?;

    let frequencies: Vec<f32> = hand_strategy
        .iter()
        .map(|v| v.as_f64().unwrap_or(0.0) as f32)
        .collect();

    Ok((position, actions, frequencies))
}

fn print_all_strategy_paths(json_path: &str) -> Result<(), String> {
    // Lire et parser le fichier JSON
    let mut file = File::open(json_path)
        .map_err(|e| format!("Erreur lors de l'ouverture du fichier: {}", e))?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| format!("Erreur lors de la lecture du fichier: {}", e))?;
    let json: Value = serde_json::from_str(&content)
        .map_err(|e| format!("Erreur lors du parsing JSON: {}", e))?;

    // Récupérer les chemins récursivement
    let mut paths = Vec::new();
    find_paths(&json, Vec::new(), &mut paths);

    // Afficher les résultats regroupés
    println!("=== RÉSUMÉ DES CHEMINS DE STRATÉGIE ===");

    // Regrouper par type de noeud final
    let mut terminal_paths = Vec::new();
    let mut chance_paths = Vec::new();

    for (path, node_type) in paths {
        if node_type == "terminal_node" {
            terminal_paths.push(path);
        } else if node_type == "chance_node" {
            chance_paths.push(path);
        }
    }

    println!("\n=== CHEMINS TERMINAUX ({}) ===", terminal_paths.len());
    for path in terminal_paths {
        println!(" → {}", path.join(" → "));
    }

    println!("\n=== CHEMINS DE CHANCE ({}) ===", chance_paths.len());
    for path in chance_paths {
        println!(" → {}", path.join(" → "));
    }

    Ok(())
}

fn find_paths(node: &Value, current_path: Vec<String>, paths: &mut Vec<(Vec<String>, String)>) {
    // Vérifier si c'est un nœud terminal ou chance
    if let Some(node_type) = node.get("node_type").and_then(|v| v.as_str()) {
        if node_type == "terminal_node" || node_type == "chance_node" {
            paths.push((current_path, node_type.to_string()));
            return;
        }
    }

    // Explorer les enfants
    if let Some(children) = node.get("childrens").and_then(|v| v.as_object()) {
        for (action, child) in children {
            let mut new_path = current_path.clone();
            new_path.push(action.clone());
            find_paths(child, new_path, paths);
        }
    }
}
