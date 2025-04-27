use crate::holes_to_strings;
use crate::Card;
use crate::PostFlopGame;
use serde_json::json;
use std::collections::HashMap;

// Fonction utilitaire pour convertir une carte en chaîne simple
pub fn card_to_string_simple(card: Card) -> String {
    let rank_chars = [
        '2', '3', '4', '5', '6', '7', '8', '9', 'T', 'J', 'Q', 'K', 'A',
    ];
    let suit_chars = ['c', 'd', 'h', 's'];

    let rank = (card >> 2) as usize;
    let suit = (card & 3) as usize;

    if rank < rank_chars.len() && suit < suit_chars.len() {
        format!("{}{}", rank_chars[rank], suit_chars[suit])
    } else {
        "??".to_string()
    }
}

// Fonction pour calculer la moyenne pondérée
pub fn compute_average(values: &[f32], weights: &[f32]) -> f32 {
    let mut sum = 0.0;
    let mut weight_sum = 0.0;
    for (&value, &weight) in values.iter().zip(weights.iter()) {
        sum += value * weight;
        weight_sum += weight;
    }
    if weight_sum > 0.0 {
        sum / weight_sum
    } else {
        0.0
    }
}

pub fn get_node_statistics(game: &mut PostFlopGame) -> HashMap<String, serde_json::Value> {
    let mut stats = HashMap::new();

    // Récupérer les informations de mise et de pot
    let total_bet_amount = game.total_bet_amount();
    let pot_base = game.tree_config().starting_pot + total_bet_amount.iter().min().unwrap();

    // Calculer les tailles de pot pour chaque joueur
    let pot_oop = (pot_base + total_bet_amount[0]) as f32;
    let pot_ip = (pot_base + total_bet_amount[1]) as f32;

    stats.insert("pot_oop".to_string(), json!(pot_oop));
    stats.insert("pot_ip".to_string(), json!(pot_ip));

    // Obtenir et filtrer les poids des mains (ignorer les combos très rares)
    let trunc = |&w: &f32| if w < 0.0005 { 0.0 } else { w };
    let weights = [
        game.weights(0).iter().map(trunc).collect::<Vec<_>>(),
        game.weights(1).iter().map(trunc).collect::<Vec<_>>(),
    ];

    // Vérifier si les ranges sont vides
    let is_empty = |player: usize| weights[player].iter().all(|&w| w == 0.0);
    let oop_empty = is_empty(0);
    let ip_empty = is_empty(1);

    stats.insert("oop_range_empty".to_string(), json!(oop_empty));
    stats.insert("ip_range_empty".to_string(), json!(ip_empty));

    // Si au moins un joueur a une range non vide
    if !oop_empty || !ip_empty {
        // Mettre à jour les poids normalisés
        game.cache_normalized_weights();

        // Récupérer les informations de stratégie
        let current_player = if game.is_terminal_node() || game.is_chance_node() {
            None
        } else {
            Some(game.current_player())
        };

        // Ajouter l'information sur le joueur actuel
        if let Some(player) = current_player {
            let player_str = if player == 0 { "OOP" } else { "IP" };
            stats.insert("current_player".to_string(), json!(player_str));
        } else if game.is_terminal_node() {
            stats.insert("current_player".to_string(), json!("terminal"));
        } else if game.is_chance_node() {
            stats.insert("current_player".to_string(), json!("chance"));
        }

        // Si c'est un nœud d'action, calculer et ajouter la stratégie
        if !game.is_terminal_node() && !game.is_chance_node() {
            let player = game.current_player();
            let range = game.private_cards(player);
            let range_size = range.len();
            let strategy_array = game.strategy();
            let hand_strings = holes_to_strings(range).unwrap();

            let mut strategy_map = HashMap::new();
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

            // Construire un mapping de stratégie par main
            for (h_idx, hand) in hand_strings.iter().enumerate().take(20) {
                let mut hand_strategy = Vec::new();

                for (a_idx, action) in action_strings.iter().enumerate() {
                    let strat_index = h_idx + a_idx * range_size;
                    let strat_value = if strat_index < strategy_array.len() {
                        strategy_array[strat_index]
                    } else {
                        0.0
                    };

                    hand_strategy.push((action.clone(), strat_value));
                }

                strategy_map.insert(hand.clone(), hand_strategy);
            }

            stats.insert("strategy_by_hand".to_string(), json!(strategy_map));

            // Calculer l'EV moyenne par action
            let normalized_weights = game.normalized_weights(player);
            let ev_details = game.expected_values_detail(player);
            let actions_len = actions.len();
            let mut action_evs = Vec::new();

            for a_idx in 0..actions_len {
                let ev_slice = &ev_details[a_idx * range_size..(a_idx + 1) * range_size];
                let avg_ev = weighted_average(ev_slice, &normalized_weights);

                action_evs.push((action_strings[a_idx].clone(), avg_ev));
            }

            stats.insert("action_evs".to_string(), json!(action_evs));
        }

        // Ajouter les équités moyennes et les EVs pour les deux joueurs
        for player in 0..2 {
            let player_str = if player == 0 { "oop" } else { "ip" };
            let normalized_weights = game.normalized_weights(player);

            if !is_empty(player) {
                let equity = game.equity(player);
                let ev = game.expected_values(player);

                let avg_equity = compute_average(&equity, &normalized_weights);
                let avg_ev = compute_average(&ev, &normalized_weights);

                stats.insert(format!("{}_equity", player_str), json!(avg_equity));
                stats.insert(format!("{}_ev", player_str), json!(avg_ev));

                // Calculer le ratio EV/equity
                let pot = if player == 0 { pot_oop } else { pot_ip };
                if avg_equity > 0.0001 {
                    let eqr = avg_ev / (pot * avg_equity);
                    stats.insert(format!("{}_eqr", player_str), json!(eqr));
                }
            }
        }
    }

    stats
}

// Fonction utilitaire pour calculer la moyenne pondérée
pub fn weighted_average(values: &[f32], weights: &[f32]) -> f32 {
    let mut sum = 0.0;
    let mut weight_sum = 0.0;

    for (&value, &weight) in values.iter().zip(weights.iter()) {
        sum += value * weight;
        weight_sum += weight;
    }

    if weight_sum > 0.0 {
        sum / weight_sum
    } else {
        0.0
    }
}
