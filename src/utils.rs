use crate::holes_to_strings;
use crate::Action;
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

#[inline]
fn round(value: f64) -> f64 {
    if value < 1.0 {
        (value * 1000000.0).round() / 1000000.0
    } else if value < 10.0 {
        (value * 100000.0).round() / 100000.0
    } else if value < 100.0 {
        (value * 10000.0).round() / 10000.0
    } else if value < 1000.0 {
        (value * 1000.0).round() / 1000.0
    } else if value < 10000.0 {
        (value * 100.0).round() / 100.0
    } else {
        (value * 10.0).round() / 10.0
    }
}

#[inline]
fn round_iter<'a>(iter: impl Iterator<Item = &'a f32> + 'a) -> impl Iterator<Item = f64> + 'a {
    iter.map(|&x| round(x as f64))
}

pub fn get_results(game: &mut PostFlopGame) -> Box<[f64]> {
    let mut buf = Vec::new();

    let total_bet_amount = game.total_bet_amount();
    let pot_base = (game.tree_config().starting_pot as f64)  // Convertir en f64
    + total_bet_amount
        .iter()
        .fold(0.0f64, |a, b| a.min(*b as f64));

    buf.push(pot_base + (total_bet_amount[0] as f64));
    buf.push(pot_base + (total_bet_amount[1] as f64));

    let trunc = |&w: &f32| if w < 0.0005 { 0.0 } else { w };
    let weights = [
        game.weights(0).iter().map(trunc).collect::<Vec<_>>(),
        game.weights(1).iter().map(trunc).collect::<Vec<_>>(),
    ];

    let is_empty = |player: usize| weights[player].iter().all(|&w| w == 0.0);
    let is_empty_flag = is_empty(0) as usize + 2 * is_empty(1) as usize;
    buf.push(is_empty_flag as f64);

    buf.extend(round_iter(weights[0].iter()));
    buf.extend(round_iter(weights[1].iter()));

    if is_empty_flag > 0 {
        buf.extend(round_iter(weights[0].iter()));
        buf.extend(round_iter(weights[1].iter()));
    } else {
        game.cache_normalized_weights();

        buf.extend(round_iter(game.normalized_weights(0).iter()));
        buf.extend(round_iter(game.normalized_weights(1).iter()));

        let equity = [game.equity(0), game.equity(1)];
        let ev = [game.expected_values(0), game.expected_values(1)];

        buf.extend(round_iter(equity[0].iter()));
        buf.extend(round_iter(equity[1].iter()));
        buf.extend(round_iter(ev[0].iter()));
        buf.extend(round_iter(ev[1].iter()));

        for player in 0..2 {
            let pot = pot_base + (total_bet_amount[player] as f64);
            for (&eq, &ev) in equity[player].iter().zip(ev[player].iter()) {
                let (eq, ev) = (eq as f64, ev as f64);
                if eq < 5e-7 {
                    buf.push(ev / 0.0);
                } else {
                    buf.push(round(ev / (pot * eq)));
                }
            }
        }
    }

    if !game.is_terminal_node() && !game.is_chance_node() {
        buf.extend(round_iter(game.strategy().iter()));
        if is_empty_flag == 0 {
            buf.extend(round_iter(
                game.expected_values_detail(game.current_player()).iter(),
            ));
        }
    }

    buf.into_boxed_slice()
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

pub fn print_hand_details(game: &mut PostFlopGame, max_hands: usize) {
    // S'assurer que nous sommes dans un nœud valide
    if game.is_terminal_node() || game.is_chance_node() {
        println!("Ce nœud ne contient pas de stratégie (terminal ou chance)");
        return;
    }

    // Récupérer le joueur actuel
    let player = game.current_player();
    let player_str = if player == 0 { "OOP" } else { "IP" };
    println!("\n=== DÉTAILS DU NŒUD ({}) ===", player_str);

    // Récupérer les données brutes selon la structure exacte
    let result_buffer = get_results(game);

    // Définir les tailles des ranges
    let oop_range_size = game.private_cards(0).len();
    let ip_range_size = game.private_cards(1).len();
    let current_range_size = if player == 0 {
        oop_range_size
    } else {
        ip_range_size
    };

    // --- Initialiser les offsets comme dans le code existant ---
    let mut offset = 0;

    // Récupérer les en-têtes
    let pot_oop = result_buffer[offset];
    offset += 1;
    let pot_ip = result_buffer[offset];
    offset += 1;
    let is_empty_flag = result_buffer[offset] as usize;
    offset += 1;

    println!("Pot OOP: {:.2} bb", { pot_oop });
    println!("Pot IP: {:.2} bb", { pot_ip });

    // --- Calculer les offsets précisément ---

    // Skip weights (raw)
    let weights_offset = offset;
    offset += oop_range_size + ip_range_size;

    // Si les ranges ne sont pas vides
    if is_empty_flag == 0 {
        // Skip normalized weights
        let norm_weights_offset = offset;
        offset += oop_range_size + ip_range_size;

        // Skip to equity of current player
        let equity_offset = offset;
        if player == 1 {
            offset += oop_range_size; // Skip OOP equities
        }
        let player_equity_offset = offset;
        offset += current_range_size + (if player == 0 { ip_range_size } else { 0 });

        // Skip to EV of current player
        let ev_offset = offset;
        if player == 1 {
            offset += oop_range_size; // Skip OOP EVs
        }
        let player_ev_offset = offset;
        offset += current_range_size + (if player == 0 { ip_range_size } else { 0 });

        // Skip EQRs
        offset += oop_range_size + ip_range_size;

        // Strategy offset
        let strategy_offset = offset;

        // Récupérer les actions disponibles
        let actions = game.available_actions();

        // EVs per action offset
        let action_ev_offset = strategy_offset + (actions.len() * current_range_size);

        // Afficher la stratégie globale
        println!("\n=== STRATÉGIE GLOBALE ===");

        // Calculer la stratégie globale pour chaque action
        for (i, action) in actions.iter().enumerate() {
            let action_str = format!("{:?}", action)
                .to_uppercase()
                .replace("(", " ")
                .replace(")", "");

            // Calculer la fréquence moyenne pour cette action
            let mut total_freq = 0.0;
            let mut total_weight = 0.0;
            let weights = game.normalized_weights(player);

            for hand_idx in 0..current_range_size {
                let strat_idx = hand_idx + i * current_range_size;
                if strategy_offset + strat_idx < result_buffer.len() {
                    let strat_value = result_buffer[strategy_offset + strat_idx];
                    total_freq += strat_value * weights[hand_idx] as f64;
                    total_weight += weights[hand_idx] as f64;
                }
            }

            let avg_freq = if total_weight > 0.0 {
                (total_freq / total_weight) * 100.0
            } else {
                0.0
            };

            println!("  {} : {:.2}%", action_str, avg_freq);
        }

        // Récupérer les noms des mains
        if let Ok(hand_strings) = holes_to_strings(game.private_cards(player)) {
            // Déterminer combien de mains à afficher
            let hands_to_show = std::cmp::min(max_hands, hand_strings.len());

            // Afficher les détails pour chaque main
            println!("\n=== DÉTAILS PAR MAIN ===");

            for hand_idx in 0..hands_to_show {
                let hand = &hand_strings[hand_idx];

                // Vérifier que les indices sont valides
                if player_equity_offset + hand_idx >= result_buffer.len()
                    || player_ev_offset + hand_idx >= result_buffer.len()
                {
                    println!("Erreur: Indices hors limites pour la main {}", hand);
                    continue;
                }

                let equity = result_buffer[player_equity_offset + hand_idx] * 100.0;
                let ev = result_buffer[player_ev_offset + hand_idx];

                println!("\n{} {{", hand);
                println!("  eq: {:.2}%", equity);
                println!("  ev: {:.2} bb", ev);
                println!("  actions: [");

                for (action_idx, action) in actions.iter().enumerate() {
                    let action_str = format!("{:?}", action)
                        .to_uppercase()
                        .replace("(", " ")
                        .replace(")", "");

                    // Calculer les indices avec précaution
                    let strategy_idx = strategy_offset + hand_idx + action_idx * current_range_size;
                    let action_ev_idx =
                        action_ev_offset + hand_idx + action_idx * current_range_size;

                    // Vérifier que les indices sont valides
                    if strategy_idx >= result_buffer.len() {
                        println!("    {{ /* Donnée de stratégie non disponible */ }},");
                        continue;
                    }

                    let frequency = result_buffer[strategy_idx] * 100.0;

                    // L'EV de cette action pour cette main (si disponible)
                    let action_ev = if action_ev_idx < result_buffer.len() {
                        result_buffer[action_ev_idx]
                    } else {
                        // Si l'indice est invalide, utiliser 0.0
                        0.0
                    };

                    println!("    {{");
                    println!("      action: \"{}\",", action_str);
                    println!("      frequency: {:.2}%,", frequency);
                    println!("      ev: {:.2} bb", action_ev);

                    // Ajouter une virgule si ce n'est pas la dernière action
                    if action_idx < actions.len() - 1 {
                        println!("    }},");
                    } else {
                        println!("    }}");
                    }
                }

                println!("  ]");
                println!("}}");
            }

            // Afficher un message si nous n'affichons pas toutes les mains
            if hands_to_show < hand_strings.len() {
                println!(
                    "\n... et {} mains supplémentaires",
                    hand_strings.len() - hands_to_show
                );
            }
        } else {
            println!("Erreur: Impossible d'obtenir les noms des mains");
        }
    } else {
        println!(
            "\nAucune main valide dans la range actuelle (is_empty_flag = {})",
            is_empty_flag
        );
    }
}

pub fn explore_random_path(game: &mut PostFlopGame) {
    use rand::Rng;

    // Créer un générateur de nombres aléatoires
    let mut rng = rand::thread_rng();

    println!("\n===== EXPLORATION ALÉATOIRE DE L'ARBRE =====");
    println!("Démarrage à la racine");

    // S'assurer que nous commençons à la racine
    game.back_to_root();

    // Garder une trace du chemin parcouru
    let mut path = Vec::new();
    let mut node_count = 0;

    // Explorer jusqu'à atteindre un nœud terminal
    while !game.is_terminal_node() {
        node_count += 1;
        println!("\n----- NŒUD #{} -----", node_count);

        // Nœud de chance (distribution d'une carte)
        if game.is_chance_node() {
            // Déterminer l'état actuel du board
            let current_board = game.current_board();
            let current_street = if current_board.len() == 3 {
                "FLOP"
            } else if current_board.len() == 4 {
                "TURN"
            } else {
                "RIVER"
            };

            let next_street = match current_street {
                "FLOP" => "TURN",
                "TURN" => "RIVER",
                _ => "???",
            };

            println!("Nœud de chance: Distribution de la carte {}", next_street);

            // Récupérer les cartes possibles via les actions disponibles
            let actions = game.available_actions();
            let possible_cards: Vec<_> = actions
                .iter()
                .filter_map(|action| {
                    if let Action::Chance(card) = action {
                        Some(*card)
                    } else {
                        None
                    }
                })
                .collect();

            if possible_cards.is_empty() {
                println!("Erreur: Aucune carte disponible!");
                break;
            }

            // Choisir une action (carte) aléatoirement
            let card_idx = rng.gen_range(0..actions.len());
            let action = &actions[card_idx];

            // Extraire la carte de l'action
            if let Action::Chance(card) = action {
                let card_name = card_to_string_simple(*card);
                println!("Carte choisie: {}", card_name);
                path.push(format!("DEAL {}", card_name));
            } else {
                println!("Erreur: Action inattendue dans un nœud de chance");
                break;
            }

            // Jouer l'action (distribuer la carte)
            game.play(card_idx);

            // Afficher le nouveau board après distribution
            let board = game.current_board();
            let board_str = board
                .iter()
                .map(|&card| card_to_string_simple(card))
                .collect::<Vec<_>>()
                .join(" ");

            println!("Nouveau board: {}", board_str);
        } else {
            // Code pour gérer les nœuds d'action
            let player = game.current_player();
            let player_str = if player == 0 { "OOP" } else { "IP" };
            println!("Joueur actuel: {} ({})", player_str, player);

            // Récupérer les actions disponibles
            let actions = game.available_actions();
            if actions.is_empty() {
                println!("Erreur: Aucune action disponible!");
                break;
            }

            // Afficher les actions disponibles
            println!("Actions disponibles:");
            for (i, action) in actions.iter().enumerate() {
                let action_str = format!("{:?}", action)
                    .to_uppercase()
                    .replace("(", " ")
                    .replace(")", "");
                println!("  {}: {}", i, action_str);
            }

            // AJOUT: Afficher les détails des mains AVANT de jouer une action
            println!("\n=== DÉTAILS DU NŒUD AVANT ACTION ===");
            print_hand_details(game, 1); // Afficher 3 mains

            // Choisir une action aléatoirement
            let action_idx = rng.gen_range(0..actions.len());
            let chosen_action = &actions[action_idx];
            let action_str = format!("{:?}", chosen_action)
                .to_uppercase()
                .replace("(", " ")
                .replace(")", "");

            println!("\nAction choisie: {} ({})", action_str, action_idx);
            path.push(action_str);

            // Jouer l'action
            game.play(action_idx);

            // // Afficher les détails du nœud APRÈS avoir joué l'action
            // if !game.is_terminal_node() && !game.is_chance_node() {
            //     println!("\n=== DÉTAILS DU NŒUD APRÈS ACTION ===");
            //     print_hand_details(game, 3);
            // }
        }
    }

    // Nœud terminal atteint
    println!("\n===== NŒUD TERMINAL ATTEINT =====");
    println!("Chemin parcouru: {}", path.join(" → "));

    // Afficher les résultats du nœud terminal
    let total_bet = game.total_bet_amount();
    let pot_base = game.tree_config().starting_pot as f32;
    let common_bet = total_bet[0].min(total_bet[1]) as f32;
    let extra_bet = (total_bet[0].max(total_bet[1]) - common_bet as i32) as f32;
    let pot_size = pot_base + 2.0 * common_bet + extra_bet;
    println!("Pot final: {:.2} bb", pot_size);

    // Afficher qui est le gagnant dans ce nœud terminal
    if let Ok(equity) = get_showdown_equity(game) {
        println!("Équité au showdown:");
        println!("  OOP: {:.2}%", equity[0] * 100.0);
        println!("  IP: {:.2}%", equity[1] * 100.0);
    } else {
        // Si un joueur a fold, son équité est 0
        if total_bet[0] > total_bet[1] {
            println!("IP a abandonné (fold)");
            println!("  OOP gagne 100% du pot");
        } else if total_bet[1] > total_bet[0] {
            println!("OOP a abandonné (fold)");
            println!("  IP gagne 100% du pot");
        } else {
            println!("Situation de showdown mais impossible de calculer l'équité");
        }
    }
}

// Fonction utilitaire pour calculer l'équité au showdown
fn get_showdown_equity(game: &mut PostFlopGame) -> Result<[f32; 2], &'static str> {
    if !game.is_terminal_node() {
        return Err("Ce n'est pas un nœud terminal");
    }

    let total_bet = game.total_bet_amount();
    if total_bet[0] != total_bet[1] {
        return Err("Ce n'est pas un nœud de showdown");
    }

    game.cache_normalized_weights();

    // Calculer l'équité
    let oop_equity = game.equity(0);
    let ip_equity = game.equity(1);

    // Faire une moyenne pondérée
    let oop_weights = game.normalized_weights(0);
    let ip_weights = game.normalized_weights(1);

    let mut oop_sum = 0.0;
    let mut oop_weight_sum = 0.0;
    for (i, &eq) in oop_equity.iter().enumerate() {
        oop_sum += eq * oop_weights[i];
        oop_weight_sum += oop_weights[i];
    }

    let mut ip_sum = 0.0;
    let mut ip_weight_sum = 0.0;
    for (i, &eq) in ip_equity.iter().enumerate() {
        ip_sum += eq * ip_weights[i];
        ip_weight_sum += ip_weights[i];
    }

    let oop_avg = if oop_weight_sum > 0.0 {
        oop_sum / oop_weight_sum
    } else {
        0.0
    };
    let ip_avg = if ip_weight_sum > 0.0 {
        ip_sum / ip_weight_sum
    } else {
        0.0
    };

    Ok([oop_avg, ip_avg])
}
