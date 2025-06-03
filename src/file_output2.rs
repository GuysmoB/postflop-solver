use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::{
    card_to_string_simple, format_path_string, holes_to_strings, play, select_spot, GameState,
    PostFlopGame, SpecificResultData, Spot, SpotType,
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
    weight: f64,
    equity: f64,
    ev: f64,
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
        3 => "flop",
        4 => "turn",
        5 => "river",
        _ => "unknown",
    };
    let current_street = match game.current_board().len() {
        3 => "F",
        4 => "T",
        5 => "R",
        _ => "F",
    };

    let street_dir = format!("{}/{}", output_dir, street_name);

    // println!(
    //     "Début de l'exploration des ranges sur {} (profondeur max: {})",
    //     street_name.to_uppercase(),
    //     max_depth
    // );
    // println!("Les fichiers seront sauvegardés dans: {}", street_dir);

    // Créer le répertoire de sortie s'il n'existe pas
    if !Path::new(&street_dir).exists() {
        std::fs::create_dir_all(&street_dir)
            .map_err(|e| format!("Échec de création du répertoire {}: {}", street_dir, e))?;
    }

    let mut state = GameState::new();
    let root_spot = Spot {
        spot_type: SpotType::Root,
        index: 0,
        player: format!("{}", street_name),
        selected_index: -1,
        actions: Vec::new(),
        cards: Vec::new(),
        pot: game.tree_config().starting_pot as f64,
        stack: game.tree_config().effective_stack as f64,
        equity_oop: 0.0,
        prev_player: None,
    };

    state.spots.push(root_spot);

    let results = select_spot(game, &mut state, 1, true, false)?;
    let root_path_id = format!("{}_Root", current_street);
    save_node_data(game, &root_path_id, &street_dir, &results)?;

    let mut actions = Vec::new();
    explore_actions_recursive(
        game,
        &mut state,
        &mut actions,
        current_street,
        &street_dir,
        0,
        max_depth,
    )
    // Ok(())
}

fn explore_actions_recursive(
    game: &mut PostFlopGame,
    state: &mut GameState,
    actions: &mut Vec<String>, // Un seul vecteur d'actions
    current_street: &str,
    output_dir: &str,
    depth: usize,
    max_depth: usize,
) -> Result<(), String> {
    // Si nous avons atteint un nœud terminal, un nœud chance ou la profondeur maximale, nous nous arrêtons
    if game.is_terminal_node() || game.is_chance_node() || depth >= max_depth {
        return Ok(());
    }

    // Obtenir le spot actuel et ses actions disponibles
    let spot_index = state.selected_spot_index as usize;
    if spot_index >= state.spots.len() {
        return Err(format!("Index de spot invalide: {}", spot_index));
    }

    let action_count = state.spots[spot_index].actions.len();
    if action_count == 0 {
        return Ok(());
    }

    let action_info: Vec<_> = state.spots[spot_index]
        .actions
        .iter()
        .map(|action| (action.name.clone(), action.amount.clone()))
        .collect();

    let history = game.cloned_history();
    let original_state = state.clone();

    for action_idx in 0..action_count {
        let (action_name, action_amount) = &action_info[action_idx];

        let action_formatted = if action_amount != "0" {
            format!("{}{}", action_name, action_amount)
        } else {
            action_name.clone()
        };

        actions.push(action_formatted);
        let results = play(game, state, action_idx)?;

        if action_name != "Fold" {
            let path_id = format_path_string(actions, current_street);
            save_node_data(game, &path_id, output_dir, &results)?;
        }

        if !game.is_terminal_node() && !game.is_chance_node() && depth + 1 < max_depth {
            explore_actions_recursive(
                game,
                state,
                actions,
                current_street,
                output_dir,
                depth + 1,
                max_depth,
            )?;
        }

        // Revenir à l'état précédent
        game.apply_history(&history);
        *state = original_state.clone();

        // Retirer la dernière action
        actions.pop();
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

pub fn format_range_string(hands_with_weights: &[(String, f64)]) -> String {
    let mut range = String::new();

    for (i, (hand, weight)) in hands_with_weights.iter().enumerate() {
        if i > 0 {
            range.push(',');
        }
        range.push_str(&format!("{}:{:.4}", hand, weight));
    }

    range
}

pub fn save_node_data(
    game: &mut PostFlopGame,
    path_id: &str,
    output_dir: &str,
    results: &SpecificResultData,
) -> Result<bool, String> {
    if results.equity.is_empty() || results.ev.is_empty() || results.weights.is_empty() {
        return Err("Résultats incomplets, impossible de sauvegarder le nœud".to_string());
    }

    let mut filename = path_id
        .replace(":", "_")
        .replace(" ", "_")
        .replace(",", "_")
        .replace("-", "_");

    filename = filename
        .chars()
        .filter(|c| !c.is_digit(10)) // Supprime tous les chiffres (base 10)
        .collect::<String>();

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

    // Créer les données des joueurs
    let range_data = RangeData {
        path_id: path_id.to_string(),
        board_size: board.len(),
        board: board_str,
        pot_oop,
        pot_ip,
        current_player: game.current_player(),
        oop_player: build_player_data(game, 0, results)?,
        ip_player: build_player_data(game, 1, results)?,
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

    // println!(
    //     "Données sauvegardées en JSON pour '{}' dans {}",
    //     path_id, full_path
    // );

    Ok(true)
}

fn build_player_data(
    game: &mut PostFlopGame,
    player: usize,
    results: &SpecificResultData,
) -> Result<PlayerData, String> {
    if results.equity.is_empty() {
        return Err("Les données d'équité sont vides".to_string());
    }

    if results.ev.is_empty() {
        return Err("Les données d'EV sont vides".to_string());
    }

    if results.weights.is_empty() {
        return Err("Les données de poids sont vides".to_string());
    }

    // Vérifier si l'indice du joueur est valide
    if player >= results.equity.len() {
        return Err(format!(
            "Index de joueur invalide: {} (max: {})",
            player,
            results.equity.len() - 1
        ));
    }

    let equity = &results.equity[player];
    let ev = &results.ev[player];
    let weights = &results.weights[player];
    let hands = if player == 0 {
        &results.oop_cards
    } else {
        &results.ip_cards
    };

    // println!(
    //     "Debug - tailles: hands={}, weights={}, equity={}, ev={}",
    //     hands.len(),
    //     weights.len(),
    //     equity.len(),
    //     ev.len()
    // );

    // S'assurer que nous n'itérons pas plus loin que la plus petite taille
    let min_len = hands
        .len()
        .min(weights.len())
        .min(equity.len())
        .min(ev.len());

    // if min_len != hands.len() {
    //     println!("ALERTE: Incohérence dans les tailles des données");
    // }

    // Convertir les mains en chaînes
    let hand_strings = match holes_to_strings(
        hands
            .iter()
            .map(|&(c1, c2)| (c1 as u8, c2 as u8))
            .collect::<Vec<_>>()
            .as_slice(),
    ) {
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

    let mut hands_with_weights = Vec::new();
    let mut hand_data = Vec::new();
    let range_size = hands.len();

    // println!("before loop: {}", range_size);

    for i in 0..hands.len() {
        // Utiliser seulement les mains avec un poids > 0
        if weights[i] <= 0.0 {
            continue;
        }

        // Utiliser les noms de mains provenant de holes_to_strings
        let hand_name = &hand_strings[i];
        hands_with_weights.push((hand_name.clone(), weights[i]));

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
            equity: if i < equity.len() { equity[i] } else { 0.0 }, // SÉCURITÉ
            ev: if i < ev.len() { ev[i] } else { 0.0 },
            strategy: hand_strategy,
        });
    }

    // println!("after loop: {}", range_size);

    let range_string = format_range_string(&hands_with_weights);

    Ok(PlayerData {
        hands_count: hand_data.len(), // Nombre de mains avec poids > 0
        hands: hand_data,
        range_string: range_string.to_string(),
    })
}
