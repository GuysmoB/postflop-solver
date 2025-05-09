use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Write};
use std::path::Path;

use crate::{
    card_to_string_simple, format_hand_cards, format_path_string, holes_to_strings, PostFlopGame,
};

#[derive(Serialize, Deserialize)]
struct HandStrategy {
    actions: Vec<String>,
    frequencies: Vec<f32>,
    ev: Vec<f32>,
}

// Structure HandData modifiée avec stratégie intégrée
#[derive(Serialize, Deserialize)]
struct HandData {
    hand: String,
    weight: f32,
    equity: f32,
    ev: f32,
    strategy: Option<HandStrategy>, // Optionnel car pas toujours disponible
}

#[derive(Serialize, Deserialize)]
pub struct PlayerData {
    hands_count: usize,
    hands: Vec<HandData>,
    range_string: String,
}

#[derive(Serialize, Deserialize)]
struct RangeData {
    path_id: String,
    board_size: usize,
    board: String,
    pot_oop: f64,
    pot_ip: f64,
    current_player: usize,
    oop_player: PlayerData,
    ip_player: PlayerData,
}

pub fn explore_and_save_ranges(
    game: &mut PostFlopGame,
    output_dir: &str,
    max_depth: usize,
) -> Result<(), String> {
    let board_size = game.current_board().len();
    let street_name = match board_size {
        3 => "FLOP",
        4 => "TURN",
        5 => "RIVER",
        _ => "UNKNOWN",
    };

    println!(
        "Début de l'exploration des ranges sur {} (profondeur max: {})",
        street_name, max_depth
    );

    // Créer le répertoire de sortie s'il n'existe pas
    if !Path::new(output_dir).exists() {
        std::fs::create_dir_all(output_dir)
            .map_err(|e| format!("Échec de création du répertoire {}: {}", output_dir, e))?;
    }

    // Sauvegarder les données du nœud racine
    let root_path_id = format!("{}_ROOT", street_name);
    save_node_data(game, &root_path_id, output_dir)?;

    // Commencer l'exploration récursive
    let mut flop_actions = Vec::new();
    let mut turn_actions = Vec::new();
    let mut river_actions = Vec::new();

    let current_street = match game.current_board().len() {
        3 => "F",
        4 => "T",
        5 => "R",
        _ => "F", // par défaut flop
    };

    explore_actions_recursive(
        game,
        &mut flop_actions,
        &mut turn_actions,
        &mut river_actions,
        current_street,
        output_dir,
        0,
        max_depth,
    )
}

fn explore_actions_recursive(
    game: &mut PostFlopGame,
    flop_actions: &mut Vec<String>,
    turn_actions: &mut Vec<String>,
    river_actions: &mut Vec<String>,
    current_street: &str,
    output_dir: &str,
    depth: usize,
    max_depth: usize,
) -> Result<(), String> {
    // Si nous avons atteint un nœud terminal, un nœud chance ou la profondeur maximale, nous nous arrêtons
    if game.is_terminal_node() || game.is_chance_node() || depth >= max_depth {
        return Ok(());
    }

    // Obtenir les actions disponibles
    let actions = game.available_actions();
    if actions.is_empty() {
        return Ok(());
    }

    // Sauvegarder l'histoire actuelle pour pouvoir y revenir
    let history = game.cloned_history();

    // Explorer chaque action
    for (action_idx, action) in actions.iter().enumerate() {
        let action_str = format!("{:?}", action);
        let action_formatted = format_action_string(&action_str);

        // Ajouter l'action au vecteur approprié selon la street
        match current_street {
            "F" => flop_actions.push(action_formatted.clone()),
            "T" => turn_actions.push(action_formatted.clone()),
            "R" => river_actions.push(action_formatted.clone()),
            _ => flop_actions.push(action_formatted.clone()), // Par défaut flop
        };

        // Jouer l'action
        game.play(action_idx);

        // Générer le path_id pour cette séquence d'actions
        let path_id = format_path_string(flop_actions, turn_actions, river_actions);

        // Sauvegarder les données du nœud actuel
        save_node_data(game, &path_id, output_dir)?;

        // Continuer l'exploration récursive si nous ne sommes pas à un nœud terminal ou chance
        if !game.is_terminal_node() && !game.is_chance_node() && depth + 1 < max_depth {
            // Si nous sommes toujours sur un nœud de joueur, explorer plus profondément
            explore_actions_recursive(
                game,
                flop_actions,
                turn_actions,
                river_actions,
                current_street,
                output_dir,
                depth + 1,
                max_depth,
            )?;
        }

        // Revenir à l'état précédent
        game.apply_history(&history);

        // Retirer l'action du vecteur
        match current_street {
            "F" => flop_actions.pop(),
            "T" => turn_actions.pop(),
            "R" => river_actions.pop(),
            _ => flop_actions.pop(),
        };
    }

    Ok(())
}

pub fn format_action_string(action: &str) -> String {
    // Supposons que format!("{:?}", action) donne "Fold", "Check", "Call", "Bet(10)", etc.
    let action_str = action.to_string();

    if action_str.contains("Fold") {
        "fold".to_string()
    } else if action_str.contains("Check") {
        "check".to_string()
    } else if action_str.contains("Call") {
        "call".to_string()
    } else if action_str.contains("Bet") {
        // Extraire le montant entre parenthèses
        if let Some(start) = action_str.find('(') {
            if let Some(end) = action_str.find(')') {
                let amount = &action_str[start + 1..end];
                format!("bet{}", amount)
            } else {
                "bet".to_string()
            }
        } else {
            "bet".to_string()
        }
    } else if action_str.contains("Raise") {
        // Extraire le montant entre parenthèses
        if let Some(start) = action_str.find('(') {
            if let Some(end) = action_str.find(')') {
                let amount = &action_str[start + 1..end];
                format!("raise{}", amount)
            } else {
                "raise".to_string()
            }
        } else {
            "raise".to_string()
        }
    } else if action_str.contains("AllIn") {
        // Extraire le montant entre parenthèses
        if let Some(start) = action_str.find('(') {
            if let Some(end) = action_str.find(')') {
                let amount = &action_str[start + 1..end];
                format!("allin{}", amount)
            } else {
                "allin".to_string()
            }
        } else {
            "allin".to_string()
        }
    } else {
        "unknown".to_string()
    }
}

pub fn extract_updated_ranges(game: &mut PostFlopGame) -> Result<(String, String), String> {
    // Assurez-vous que les poids normalisés sont à jour
    game.cache_normalized_weights();

    // Extraire les mains et les poids pour chaque joueur
    let oop_cards = game.private_cards(0);
    let ip_cards = game.private_cards(1);
    let oop_weights = game.normalized_weights(0);
    let ip_weights = game.normalized_weights(1);

    // Calculer les sommes totales des poids positifs
    let oop_total: f32 = oop_weights.iter().filter(|&&w| w > 0.0).sum();
    let ip_total: f32 = ip_weights.iter().filter(|&&w| w > 0.0).sum();

    // Initialiser les chaînes de résultat
    let mut oop_range = String::new(); // Cette ligne manquait!
    let mut ip_range = String::new(); // Cette ligne manquait!

    // Conversion pour OOP (villain)
    let mut oop_hands: Vec<(String, f32)> = Vec::new();
    for (idx, &(card1, card2)) in oop_cards.iter().enumerate() {
        if oop_weights[idx] > 0.0 {
            // Normaliser le poids en pourcentage du total
            let normalized_weight = if oop_total > 0.0 {
                (oop_weights[idx] / oop_total) * 100.0
            } else {
                0.0
            };

            let hand_str = format_hand_cards((card1, card2));
            oop_hands.push((hand_str, normalized_weight));
        }
    }

    // Même logique pour IP
    let mut ip_hands: Vec<(String, f32)> = Vec::new();
    for (idx, &(card1, card2)) in ip_cards.iter().enumerate() {
        if ip_weights[idx] > 0.0 {
            let normalized_weight = if ip_total > 0.0 {
                (ip_weights[idx] / ip_total) * 100.0
            } else {
                0.0
            };

            let hand_str = format_hand_cards((card1, card2));
            ip_hands.push((hand_str, normalized_weight));
        }
    }

    // Convertir les vecteurs en strings de range
    for (hand, weight) in oop_hands {
        if !oop_range.is_empty() {
            oop_range.push(',');
        }
        oop_range.push_str(&format!("{}:{:.2}", hand, weight));
    }

    for (hand, weight) in ip_hands {
        if !ip_range.is_empty() {
            ip_range.push(',');
        }
        ip_range.push_str(&format!("{}:{:.2}", hand, weight));
    }

    Ok((oop_range, ip_range))
}

pub fn save_node_data(
    game: &mut PostFlopGame,
    path_id: &str,
    output_dir: &str,
) -> Result<bool, String> {
    let filename = path_id
        .replace(":", "_")
        .replace(" ", "_")
        .replace(",", "_")
        .replace("-", "_");

    let full_path = format!("{}/{}.json", output_dir, filename);

    game.cache_normalized_weights();

    // Créer une structure RangeData
    let board = game.current_board();
    let board_str = board
        .iter()
        .map(|&c| card_to_string_simple(c))
        .collect::<Vec<_>>()
        .join(" ");

    // Calculer le pot
    let total_bet_amount = game.total_bet_amount();
    let pot_base = game.tree_config().starting_pot as f64
        + (total_bet_amount[0].min(total_bet_amount[1]) as f64);
    let pot_oop = pot_base + total_bet_amount[0] as f64;
    let pot_ip = pot_base + total_bet_amount[1] as f64;

    // Extraire les ranges à jour
    let (oop_range, ip_range) = extract_updated_ranges(game)?;

    // Créer les données des joueurs
    let range_data = RangeData {
        path_id: path_id.to_string(),
        board_size: board.len(),
        board: board_str,
        pot_oop,
        pot_ip,
        current_player: game.current_player(),
        oop_player: build_player_data(game, 0, &oop_range)?,
        ip_player: build_player_data(game, 1, &ip_range)?,
    };

    // Sérialiser en JSON
    let json_data = serde_json::to_string_pretty(&range_data)
        .map_err(|e| format!("Erreur de sérialisation JSON: {}", e))?;

    // Créer le répertoire s'il n'existe pas
    if !Path::new(output_dir).exists() {
        std::fs::create_dir_all(output_dir)
            .map_err(|e| format!("Échec de création du répertoire {}: {}", output_dir, e))?;
    }

    // Écrire le fichier JSON
    std::fs::write(&full_path, json_data)
        .map_err(|e| format!("Échec d'écriture du fichier {}: {}", full_path, e))?;

    println!(
        "Données sauvegardées en JSON pour '{}' dans {}",
        path_id, full_path
    );

    Ok(true)
}

fn build_player_data(
    game: &mut PostFlopGame,
    player: usize,
    range_string: &str,
) -> Result<PlayerData, String> {
    let equity = game.equity(player);
    let ev = game.expected_values(player);
    let weights = game.normalized_weights(player);
    let hands = game.private_cards(player);

    // Utiliser holes_to_strings comme dans display_top_hands
    let hand_strings = match holes_to_strings(hands) {
        Ok(strings) => strings,
        Err(_) => return Err("Erreur lors de la conversion des mains en chaînes".to_string()),
    };

    let is_current_player = player == game.current_player();

    // Si nous sommes sur un nœud joueur actif, récupérer les données de stratégie
    let mut action_names = Vec::new();
    let mut strategy = Vec::new();
    let mut action_evs = Vec::new();

    if is_current_player && !game.is_terminal_node() && !game.is_chance_node() {
        let actions = game.available_actions();
        action_names = actions
            .iter()
            .map(|a| format!("{:?}", a)) // Format standard pour les actions
            .collect();
        strategy = game.strategy();
        action_evs = game.expected_values_detail(player);
    }

    let mut hand_data = Vec::new();
    let range_size = hands.len();

    for i in 0..hands.len() {
        // Utiliser seulement les mains avec un poids > 0
        if weights[i] <= 0.0 {
            continue;
        }

        // Utiliser les noms de mains provenant de holes_to_strings
        let hand_name = &hand_strings[i];

        let mut hand_strategy = None;

        // Ajouter la stratégie pour chaque main si c'est le joueur actif
        if is_current_player && !game.is_terminal_node() && !game.is_chance_node() {
            let mut hand_freqs = Vec::new();
            let mut hand_evs = Vec::new();

            for action_idx in 0..action_names.len() {
                let strat_idx = action_idx * range_size + i;
                if strat_idx < strategy.len() {
                    hand_freqs.push(strategy[strat_idx]);
                } else {
                    hand_freqs.push(0.0);
                }

                let ev_idx = action_idx * range_size + i;
                if ev_idx < action_evs.len() {
                    hand_evs.push(action_evs[ev_idx]);
                } else {
                    hand_evs.push(0.0);
                }
            }

            hand_strategy = Some(HandStrategy {
                actions: action_names.clone(),
                frequencies: hand_freqs,
                ev: hand_evs,
            });
        }

        hand_data.push(HandData {
            hand: hand_name.clone(),
            weight: weights[i],
            equity: equity[i],
            ev: ev[i],
            strategy: hand_strategy,
        });
    }

    Ok(PlayerData {
        hands_count: hand_data.len(), // Nombre de mains avec poids > 0
        hands: hand_data,
        range_string: range_string.to_string(),
    })
}
