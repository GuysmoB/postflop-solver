use crate::holes_to_strings;
use crate::Action;
use crate::Card;
use crate::GameState;
use crate::PostFlopGame;
use crate::Spot;
use crate::SpotType;
use serde_json::json;
use std::collections::HashMap;

struct ResultData {
    current_player: String,
    num_actions: usize,
    is_empty: bool,
    eqr_base: [f64; 2],
    weights: Vec<Vec<f64>>,
    normalizer: Vec<Vec<f64>>,
    equity: Vec<Vec<f64>>,
    ev: Vec<Vec<f64>>,
    eqr: Vec<Vec<f64>>,
    strategy: Vec<f64>,
    action_ev: Vec<f64>,
}

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
    let player_type = if player == 0 { "oop" } else { "ip" };
    let num_actions = game.available_actions().len();
    let result_buffer = get_specific_result(game, player_type, num_actions);

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

/// Fonction pour obtenir des résultats spécifiques basés sur le type de nœud et le nombre d'actions
/// Similaire à la fonction getResults() du frontend
/// Fonction pour obtenir des résultats spécifiques basés sur le type de joueur et le nombre d'actions
/// Cette fonction reproduit exactement la structure de données utilisée par getSpecificResultsFront dans le frontend
pub fn get_specific_result(
    game: &mut PostFlopGame,
    player_type: &str, // "oop", "ip", "chance", "terminal"
    num_actions: usize,
) -> Box<[f64]> {
    // S'assurer que les poids normalisés sont disponibles
    game.cache_normalized_weights();

    // Obtenir les résultats bruts
    let buffer = get_results(game);

    // Créer un nouveau buffer qui sera retourné avec la structure exacte attendue par le frontend
    let mut offset = 0;

    // Récupérer les en-têtes (potOOP, potIP, isEmpty)
    let pot_oop = buffer[offset];
    offset += 1;

    let pot_ip = buffer[offset];
    offset += 1;

    let is_empty_flag = buffer[offset] as usize;
    offset += 1;

    let is_empty = is_empty_flag != 0;
    let eqr_base = [pot_oop, pot_ip];

    // Récupérer tailles des ranges
    let oop_range_size = game.private_cards(0).len();
    let ip_range_size = game.private_cards(1).len();

    // Poids bruts
    let mut weights: Vec<Vec<f64>> = vec![Vec::new(), Vec::new()];

    // OOP weights
    for i in 0..oop_range_size {
        weights[0].push(buffer[offset]);
        offset += 1;
    }

    // IP weights
    for i in 0..ip_range_size {
        weights[1].push(buffer[offset]);
        offset += 1;
    }

    // Poids normalisés
    let mut normalizer: Vec<Vec<f64>> = vec![Vec::new(), Vec::new()];

    // OOP normalized weights
    for i in 0..oop_range_size {
        normalizer[0].push(buffer[offset]);
        offset += 1;
    }

    // IP normalized weights
    for i in 0..ip_range_size {
        normalizer[1].push(buffer[offset]);
        offset += 1;
    }

    // Si non vide, récupérer équité, EV et EQR
    let mut equity: Vec<Vec<f64>> = vec![Vec::new(), Vec::new()];
    let mut ev: Vec<Vec<f64>> = vec![Vec::new(), Vec::new()];
    let mut eqr: Vec<Vec<f64>> = vec![Vec::new(), Vec::new()];

    if !is_empty {
        // Équité OOP
        for i in 0..oop_range_size {
            equity[0].push(buffer[offset]);
            offset += 1;
        }

        // Équité IP
        for i in 0..ip_range_size {
            equity[1].push(buffer[offset]);
            offset += 1;
        }

        // EV OOP
        for i in 0..oop_range_size {
            ev[0].push(buffer[offset]);
            offset += 1;
        }

        // EV IP
        for i in 0..ip_range_size {
            ev[1].push(buffer[offset]);
            offset += 1;
        }

        // EQR OOP
        for i in 0..oop_range_size {
            eqr[0].push(buffer[offset]);
            offset += 1;
        }

        // EQR IP
        for i in 0..ip_range_size {
            eqr[1].push(buffer[offset]);
            offset += 1;
        }
    }

    // Si c'est un nœud joueur (oop ou ip), récupérer la stratégie et les EV par action
    let mut strategy: Vec<f64> = Vec::new();
    let mut action_ev: Vec<f64> = Vec::new();

    if player_type == "oop" || player_type == "ip" {
        let current_range_size = if player_type == "oop" {
            oop_range_size
        } else {
            ip_range_size
        };

        // Stratégie
        for _ in 0..(num_actions * current_range_size) {
            if offset < buffer.len() {
                strategy.push(buffer[offset]);
                offset += 1;
            } else {
                strategy.push(0.0);
            }
        }

        // EV par action (seulement si !isEmpty)
        if !is_empty {
            for _ in 0..(num_actions * current_range_size) {
                if offset < buffer.len() {
                    action_ev.push(buffer[offset]);
                    offset += 1;
                } else {
                    action_ev.push(0.0);
                }
            }
        }
    }

    // Créer un dictionnaire pour débogage
    let results = ResultData {
        current_player: player_type.to_string(),
        num_actions,
        is_empty,
        eqr_base,
        weights,
        normalizer,
        equity,
        ev,
        eqr,
        strategy,
        action_ev,
    };

    // Afficher les résultats pour débogage
    println!("Type de joueur: {}", player_type);
    println!("Nombre d'actions: {}", num_actions);
    println!("Est vide: {}", is_empty);
    println!("Taille OOP range: {}", oop_range_size);
    println!("Taille IP range: {}", ip_range_size);

    // Retourner le buffer initial, car il contient déjà toutes les données nécessaires
    // exactement dans le format attendu par le frontend
    buffer
}

// Structure pour organiser les résultats (pour débogage)

/// Fonction pour naviguer vers un nœud spécifique dans l'arbre de jeu
/// Similaire à la fonction selectSpotFront() du frontend
pub fn select_spot_old(
    game: &mut PostFlopGame,
    spot_index: usize,
    history: &[String],
    print_details: bool,
) -> Result<(), String> {
    // Revenir à la racine de l'arbre
    game.back_to_root();

    // Si spot_index est 0, on est déjà à la racine
    if spot_index == 0 {
        println!("Revenu à la racine de l'arbre");
        return Ok(());
    }

    // Pour chaque action jusqu'à l'index souhaité
    for (i, action_str) in history.iter().take(spot_index).enumerate() {
        // Vérifier si nous sommes dans un nœud de chance (pour les cartes)
        if game.is_chance_node() {
            // Traitement des nœuds de chance
            if action_str.starts_with("DEAL ") {
                let card_str = &action_str[5..]; // Extraire le nom de la carte

                let actions = game.available_actions();
                let mut found = false;

                // Trouver l'index de l'action correspondant à cette carte
                for (idx, action) in actions.iter().enumerate() {
                    if let Action::Chance(card) = action {
                        let card_name = card_to_string_simple(*card);
                        if card_name == card_str {
                            game.play(idx);
                            found = true;
                            break;
                        }
                    }
                }

                if !found {
                    return Err(format!("Carte {} non disponible", card_str));
                }
            } else {
                return Err(format!(
                    "Action inattendue {} dans un nœud de chance",
                    action_str
                ));
            }
        } else {
            // Traitement des nœuds d'action (joueur)
            let actions = game.available_actions();
            let mut found = false;

            // Recherche de l'action correspondante
            for (idx, action) in actions.iter().enumerate() {
                let action_name = format!("{:?}", action)
                    .to_uppercase()
                    .replace("(", " ")
                    .replace(")", "");

                if action_name == *action_str {
                    game.play(idx);
                    found = true;
                    break;
                }
            }

            if !found {
                return Err(format!("Action {} non disponible", action_str));
            }
        }

        // Afficher les détails du nœud si demandé
        if print_details && i == spot_index - 1 {
            println!("\n===== NŒUD #{} =====", i + 1);

            // Déterminer le type de joueur et le nombre d'actions
            let current_player = if game.is_terminal_node() {
                "terminal"
            } else if game.is_chance_node() {
                "chance"
            } else if game.current_player() == 0 {
                "oop"
            } else {
                "ip"
            };

            let num_actions = if game.is_terminal_node() || game.is_chance_node() {
                0
            } else {
                game.available_actions().len()
            };

            // Utiliser get_specific_result pour obtenir les résultats détaillés
            game.cache_normalized_weights(); // S'assurer que les poids sont mis en cache
            let results = get_specific_result(game, current_player, num_actions);

            // Afficher l'état du jeu en fonction du type de nœud
            if game.is_terminal_node() {
                println!("Nœud terminal");

                // Calculer le pot final
                let total_bet = game.total_bet_amount();
                let pot_base = game.tree_config().starting_pot as f32;
                let common_bet = total_bet[0].min(total_bet[1]) as f32;
                let extra_bet = (total_bet[0].max(total_bet[1]) - common_bet as i32) as f32;
                let pot_size = pot_base + 2.0 * common_bet + extra_bet;
                println!("Pot final: {:.2} bb", pot_size);

                // Afficher l'équité au showdown si applicable
                if let Ok(equity) = get_showdown_equity(game) {
                    println!("Équité au showdown:");
                    println!("  OOP: {:.2}%", equity[0] * 100.0);
                    println!("  IP: {:.2}%", equity[1] * 100.0);
                } else {
                    // Si un joueur a fold
                    if total_bet[0] > total_bet[1] {
                        println!("IP a abandonné (fold)");
                    } else if total_bet[1] > total_bet[0] {
                        println!("OOP a abandonné (fold)");
                    }
                }
            } else if game.is_chance_node() {
                println!("Nœud de chance");

                // Afficher le board actuel
                let board = game.current_board();
                let board_str = board
                    .iter()
                    .map(|&card| card_to_string_simple(card))
                    .collect::<Vec<_>>()
                    .join(" ");
                println!("Board actuel: {}", board_str);

                // Afficher les cartes disponibles
                println!("Cartes disponibles:");
                for (i, action) in game.available_actions().iter().enumerate() {
                    if let Action::Chance(card) = action {
                        println!("  {}: {}", i, card_to_string_simple(*card));
                    }
                }
            } else {
                // Nœud d'action (joueur)
                let player = game.current_player();
                let player_str = if player == 0 { "OOP" } else { "IP" };
                println!("Joueur actuel: {} ({})", player_str, player);

                // Extraire les données principales des résultats
                let is_empty = results[2] != 0.0;
                let header_offset = 3;
                let oop_range_size = game.private_cards(0).len();
                let ip_range_size = game.private_cards(1).len();

                // Calculer les offsets pour accéder aux différentes sections des résultats
                let weight_offset = header_offset;
                let normalizer_offset = weight_offset + oop_range_size + ip_range_size;
                let equity_offset = normalizer_offset + oop_range_size + ip_range_size;

                // Afficher les taux de stratégie globaux (similaire au frontend)
                if !is_empty {
                    println!("\n=== STRATÉGIE GLOBALE ===");
                    let actions = game.available_actions();
                    let strategy = game.strategy();
                    let weights = game.normalized_weights(player);
                    let mut total_weight = 0.0;
                    let mut action_totals = vec![0.0; actions.len()];

                    // Calculer les fréquences moyennes pondérées
                    for hand_idx in 0..weights.len() {
                        let weight = weights[hand_idx];
                        total_weight += weight;

                        for (i, _) in actions.iter().enumerate() {
                            let strat_idx = hand_idx + i * weights.len();
                            if strat_idx < strategy.len() {
                                action_totals[i] += strategy[strat_idx] * weight;
                            }
                        }
                    }

                    // Afficher les fréquences par action
                    for (i, action) in actions.iter().enumerate() {
                        let action_str = format!("{:?}", action)
                            .to_uppercase()
                            .replace("(", " ")
                            .replace(")", "");

                        let avg_freq = if total_weight > 0.0 {
                            (action_totals[i] / total_weight) * 100.0
                        } else {
                            0.0
                        };

                        println!("  {} : {:.2}%", action_str, avg_freq);
                    }

                    // Afficher les détails par main
                    print_hand_details(game, 5);
                } else {
                    println!("\nAucune main valide dans la range actuelle.");
                }
            }
        }
    }

    Ok(())
}

/// Fonction pour naviguer vers un nœud spécifique dans l'arbre de jeu
/// Basée sur selectSpotFront du frontend
pub fn select_spot(
    game: &mut PostFlopGame,
    spot_index: usize,
    need_splice: bool,
    from_deal: bool,
) -> Result<(), String> {
    // Revenir à la racine de l'arbre
    game.back_to_root();

    // Si spot_index est 0, sélectionner l'index 1 à la place
    if spot_index == 0 {
        return select_spot(game, 1, need_splice, from_deal);
    }

    // Définir les variables de suivi
    let mut selected_spot_index = spot_index;
    let mut selected_chance_index = -1;
    let mut history = Vec::new();

    // Construire l'historique des actions à partir des spots précédents
    // Dans le cas réel, cela serait basé sur le tableau spots du frontend
    for i in 1..spot_index {
        // Ici on simulerait la récupération de l'action sélectionnée
        // pour chaque spot précédent
        let action_str = format!("ACTION_{}", i); // Simulé pour l'exemple
        history.push(action_str);
    }

    // Convertir l'historique en actions jouables
    let mut action_history = Vec::new();
    for action_str in history.iter() {
        let action = if action_str.starts_with("DEAL ") {
            // Traiter les cartes
            let card_str = &action_str[5..];
            // Simuler la conversion d'une chaîne de carte en Card
            let card = 0; // Valeur fictive pour l'exemple
            Action::Chance(card)
        } else {
            // Traiter les actions de joueur
            match action_str.as_str() {
                "FOLD" => Action::Fold,
                "CHECK" => Action::Check,
                "CALL" => Action::Call,
                s if s.starts_with("BET ") => {
                    let amount: i32 = 10; // Valeur fictive pour l'exemple
                    Action::Bet(amount)
                }
                _ => return Err(format!("Action non reconnue: {}", action_str)),
            }
        };
        action_history.push(action);
    }

    // Appliquer l'historique des actions
    // game.apply_history(&action_history)?;

    // Déterminer le joueur actuel et le nombre d'actions
    let current_player = if game.is_terminal_node() {
        "terminal"
    } else if game.is_chance_node() {
        "chance"
    } else if game.current_player() == 0 {
        "oop"
    } else {
        "ip"
    };

    let num_actions = if game.is_terminal_node() || game.is_chance_node() {
        0
    } else {
        game.available_actions().len()
    };

    // Obtenir les résultats spécifiques
    game.cache_normalized_weights();
    let results = get_specific_result(game, current_player, num_actions);

    // Si need_splice est true, on simulerait ici la mise à jour du tableau spots
    if need_splice {
        println!("Mise à jour de la structure des nœuds (need_splice=true)");

        if game.is_terminal_node() {
            println!("Terminal node: mise à jour pour refléter un nœud terminal");
        } else if game.is_chance_node() {
            println!("Chance node: mise à jour pour refléter un nœud de chance");
        } else {
            println!("Player node: mise à jour pour refléter un nœud de joueur");
        }
    }

    // Afficher les informations sur le nœud actuel
    println!("\n===== NŒUD #{} =====", spot_index);
    println!("Type de joueur: {}", current_player);

    // Afficher les détails spécifiques selon le type de nœud
    if game.is_terminal_node() {
        println!("Nœud terminal");

        // Calculer le pot final
        let total_bet = game.total_bet_amount();
        let pot_base = game.tree_config().starting_pot as f64;
        let common_bet = total_bet[0].min(total_bet[1]) as f64;
        let extra_bet = (total_bet[0].max(total_bet[1]) - common_bet as i32) as f64;
        let pot_size = pot_base + 2.0 * common_bet + extra_bet;
        println!("Pot final: {:.2} bb", pot_size);

        // Afficher l'équité au showdown si applicable
        if let Ok(equity) = get_showdown_equity(game) {
            println!("Équité au showdown:");
            println!("  OOP: {:.2}%", equity[0] * 100.0);
            println!("  IP: {:.2}%", equity[1] * 100.0);
        } else {
            // Si un joueur a fold
            if total_bet[0] > total_bet[1] {
                println!("IP a abandonné (fold)");
            } else if total_bet[1] > total_bet[0] {
                println!("OOP a abandonné (fold)");
            }
        }
    } else if game.is_chance_node() {
        println!("Nœud de chance");

        // Afficher le board actuel
        let board = game.current_board();
        let board_str = board
            .iter()
            .map(|&card| card_to_string_simple(card))
            .collect::<Vec<_>>()
            .join(" ");

        // Déterminer le street actuel
        let street_name = if board.len() == 3 {
            "FLOP → TURN"
        } else if board.len() == 4 {
            "TURN → RIVER"
        } else {
            "???"
        };

        println!("Board actuel: {} ({})", board_str, street_name);

        // Afficher les cartes disponibles
        println!("Cartes disponibles:");
        let actions = game.available_actions();
        for (i, action) in actions.iter().enumerate() {
            if let Action::Chance(card) = action {
                println!("  {}: DEAL {}", i, card_to_string_simple(*card));
            }
        }
    } else {
        // Nœud d'action (joueur)
        let player = game.current_player();
        let player_str = if player == 0 { "OOP" } else { "IP" };
        println!("Joueur actuel: {} ({})", player_str, player);

        // Afficher les actions disponibles
        let actions = game.available_actions();
        println!("Actions disponibles:");
        for (i, action) in actions.iter().enumerate() {
            let action_str = format!("{:?}", action)
                .to_uppercase()
                .replace("(", " ")
                .replace(")", "");
            println!("  {}: {}", i, action_str);
        }

        // Afficher la stratégie globale
        let is_empty = results[2] != 0.0;
        if !is_empty {
            println!("\n=== STRATÉGIE GLOBALE ===");
            let strategy = game.strategy();
            let weights = game.normalized_weights(player);
            let mut total_weight = 0.0;
            let mut action_totals = vec![0.0; actions.len()];

            // Calculer les fréquences moyennes pondérées
            for hand_idx in 0..weights.len() {
                let weight = weights[hand_idx];
                total_weight += weight as f64;

                for (i, _) in actions.iter().enumerate() {
                    let strat_idx = hand_idx + i * weights.len();
                    if strat_idx < strategy.len() {
                        action_totals[i] += (strategy[strat_idx] as f64) * (weight as f64);
                    }
                }
            }

            // Afficher les fréquences par action
            for (i, action) in actions.iter().enumerate() {
                let action_str = format!("{:?}", action)
                    .to_uppercase()
                    .replace("(", " ")
                    .replace(")", "");

                let avg_freq = if total_weight > 0.0 {
                    (action_totals[i] / total_weight) * 100.0
                } else {
                    0.0
                };

                println!("  {} : {:.2}%", action_str, avg_freq);
            }

            // Si demandé, afficher les détails par main
            print_hand_details(game, 5);
        } else {
            println!("\nAucune main valide dans la range actuelle.");
        }
    }

    Ok(())
}

/// Fonction pour explorer l'arbre de jeu de manière interactive
/// Fonction pour explorer l'arbre de jeu de manière interactive
pub fn explore_tree(game: &mut PostFlopGame) {
    use std::io::{self, Write};

    println!("\n===== EXPLORATION INTERACTIVE DE L'ARBRE =====");

    // S'assurer que nous commençons à la racine
    game.back_to_root();

    // Historique des actions pour la navigation
    let mut history = Vec::new();
    let mut spot_index = 0;

    // Variable pour stocker la carte sélectionnée dans un nœud de chance
    let mut selected_card_idx: Option<usize> = None;

    loop {
        println!("\n----- NŒUD ACTUEL (#{}) -----", spot_index);

        // Afficher l'état actuel du jeu
        if game.is_terminal_node() {
            println!("Nœud terminal");

            // Calculer le pot final
            let total_bet = game.total_bet_amount();
            let pot_base = game.tree_config().starting_pot as f32;
            let common_bet = total_bet[0].min(total_bet[1]) as f32;
            let extra_bet = (total_bet[0].max(total_bet[1]) - common_bet as i32) as f32;
            let pot_size = pot_base + 2.0 * common_bet + extra_bet;
            println!("Pot final: {:.2} bb", pot_size);

            // Afficher l'équité au showdown si applicable
            if let Ok(equity) = get_showdown_equity(game) {
                println!("Équité au showdown:");
                println!("  OOP: {:.2}%", equity[0] * 100.0);
                println!("  IP: {:.2}%", equity[1] * 100.0);
            } else {
                // Si un joueur a fold
                if total_bet[0] > total_bet[1] {
                    println!("IP a abandonné (fold)");
                } else if total_bet[1] > total_bet[0] {
                    println!("OOP a abandonné (fold)");
                }
            }
        } else if game.is_chance_node() {
            println!("Nœud de chance");

            // Afficher le board actuel
            let board = game.current_board();
            let board_str = board
                .iter()
                .map(|&card| card_to_string_simple(card))
                .collect::<Vec<_>>()
                .join(" ");
            println!("Board actuel: {}", board_str);

            // Déterminer l'étape actuelle
            let current_street = if board.len() == 3 {
                "FLOP → TURN"
            } else if board.len() == 4 {
                "TURN → RIVER"
            } else {
                "?"
            };
            println!("Distribution de: {}", current_street);

            // Afficher les actions disponibles
            let actions = game.available_actions();
            println!("Cartes disponibles:");
            for (i, action) in actions.iter().enumerate() {
                if let Action::Chance(card) = action {
                    // Marquer la carte sélectionnée avec un indicateur
                    let selection_marker = if Some(i) == selected_card_idx {
                        "✓ "
                    } else {
                        "  "
                    };
                    println!(
                        "  {}: {}DEAL {}",
                        i,
                        selection_marker,
                        card_to_string_simple(*card)
                    );
                }
            }

            // Afficher les options supplémentaires pour les nœuds de chance
            if let Some(idx) = selected_card_idx {
                if let Action::Chance(card) = actions[idx] {
                    println!("\nCarte sélectionnée: DEAL {}", card_to_string_simple(card));
                    println!("Options spéciales:");
                    println!("  c = Confirmer la sélection");
                    println!("  x = Annuler la sélection");
                }
            }
        } else {
            // Nœud d'action (joueur)
            let player = game.current_player();
            let player_str = if player == 0 { "OOP" } else { "IP" };
            println!("Joueur actuel: {} ({})", player_str, player);

            // Afficher les actions disponibles
            let actions = game.available_actions();
            println!("Actions disponibles:");
            for (i, action) in actions.iter().enumerate() {
                let action_str = format!("{:?}", action)
                    .to_uppercase()
                    .replace("(", " ")
                    .replace(")", "");
                println!("  {}: {}", i, action_str);
            }

            // Afficher les détails du nœud
            print_hand_details(game, 3);
        }

        // Afficher le chemin parcouru
        if !history.is_empty() {
            println!("\nChemin parcouru: {}", history.join(" → "));
        }

        // Menu d'options
        println!("\nOptions:");
        println!(
            "  [0-9] = {} une action",
            if game.is_chance_node() {
                "Sélectionner"
            } else {
                "Jouer"
            }
        );
        println!("  b = Retour en arrière");
        println!("  r = Revenir à la racine");
        println!("  d = Détails complets");
        println!("  q = Quitter");

        // Lire l'entrée utilisateur
        print!("> ");
        io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let input = input.trim();

        if input == "q" {
            break;
        } else if input == "r" {
            // Revenir à la racine
            game.back_to_root();
            history.clear();
            spot_index = 0;
            selected_card_idx = None;
        } else if input == "b" {
            // Retour en arrière d'un niveau
            if spot_index > 0 {
                spot_index -= 1;
                history.pop();
                selected_card_idx = None;
                select_spot_old(game, spot_index, &history, false).unwrap();
            } else {
                println!("Déjà à la racine!");
            }
        } else if input == "d" {
            // Afficher les détails complets
            if !game.is_terminal_node() && !game.is_chance_node() {
                print_hand_details(game, 10);
            } else {
                println!("Pas de détails supplémentaires disponibles pour ce nœud");
            }
        } else if input == "c" && game.is_chance_node() && selected_card_idx.is_some() {
            // Confirmer la sélection d'une carte
            let idx = selected_card_idx.unwrap();
            let actions = game.available_actions();
            if idx < actions.len() {
                if let Action::Chance(card) = actions[idx] {
                    // Ajouter la carte à l'historique
                    let action_str = format!("DEAL {}", card_to_string_simple(card));

                    // Jouer l'action
                    game.play(idx);

                    // Mettre à jour l'historique
                    history.push(action_str);
                    spot_index += 1;
                    selected_card_idx = None;
                }
            }
        } else if input == "x" && game.is_chance_node() && selected_card_idx.is_some() {
            // Annuler la sélection
            selected_card_idx = None;
        } else if let Ok(index) = input.parse::<usize>() {
            // Sélectionner ou jouer l'action selon le type de nœud
            let actions = game.available_actions();
            if index < actions.len() {
                if game.is_chance_node() {
                    // Dans un nœud de chance, sélectionner la carte sans la jouer immédiatement
                    selected_card_idx = Some(index);
                    println!("Carte sélectionnée! Tapez 'c' pour confirmer ou 'x' pour annuler.");
                } else {
                    // Dans un nœud de joueur, jouer l'action directement
                    let action = &actions[index];
                    let action_str = format!("{:?}", action)
                        .to_uppercase()
                        .replace("(", " ")
                        .replace(")", "");

                    // Jouer l'action
                    game.play(index);

                    // Mettre à jour l'historique
                    history.push(action_str);
                    spot_index += 1;
                    selected_card_idx = None;
                }
            } else {
                println!("Action invalide!");
            }
        } else {
            println!("Commande non reconnue");
        }
    }
}

/// Exécuter le scénario: OOP bet, IP call, puis turn
pub fn run_bet_call_turn_scenario(game: &mut PostFlopGame) -> Result<(), String> {
    // Créer l'état du jeu
    let mut state = GameState::new();

    // Initialiser avec la racine (flop)
    let starting_pot = game.tree_config().starting_pot as f64;
    let effective_stack = game.tree_config().effective_stack as f64;
    let board = game.current_board();
    let board_cards: Vec<usize> = board.iter().map(|&card| card as usize).collect();

    // Spot racine
    let root_spot = Spot {
        spot_type: SpotType::Root,
        index: 0,
        player: "flop".to_string(),
        selected_index: -1,
        actions: Vec::new(),
        cards: Vec::new(),
        pot: starting_pot,
        stack: effective_stack,
        equity_oop: 0.0,
    };

    state.spots.push(root_spot);

    // Sélectionner le premier spot (initialise le jeu et affiche les actions disponibles)
    select_spot(game, &mut state, 1, true, false)?;

    // À ce stade, state.spots[1] contient un nœud joueur OOP avec des actions

    println!("\n=== ÉTAPE 1: OOP BET ===");

    // Trouver l'index de l'action "Bet" pour OOP
    let bet_idx = state.spots[1].actions.iter().position(|a| a.name == "Bet");
    if let Some(bet_idx) = bet_idx {
        // Sélectionner cette action
        state.spots[1].selected_index = bet_idx as i32;
        state.spots[1].actions[bet_idx].is_selected = true;

        // Avancer au nœud suivant (IP)
        select_spot(game, &mut state, 2, true, false)?;

        println!("\n=== ÉTAPE 2: IP CALL ===");

        // Trouver l'index de l'action "Call" pour IP
        let call_idx = state.spots[2].actions.iter().position(|a| a.name == "Call");
        if let Some(call_idx) = call_idx {
            // Sélectionner cette action
            state.spots[2].selected_index = call_idx as i32;
            state.spots[2].actions[call_idx].is_selected = true;

            // Avancer au nœud suivant (nœud de chance pour la turn)
            select_spot(game, &mut state, 3, true, false)?;

            println!("\n=== ÉTAPE 3: TURN (NŒUD DE CHANCE) ===");

            // À ce stade, state.spots[3] est un nœud de chance (turn)
            // et state.spots[4] sera un nœud joueur OOP après la turn

            // Pour simuler la sélection d'une carte turn, choisissons la première carte disponible
            if let Some(card_idx) = state.spots[3].cards.iter().position(|c| !c.is_dead) {
                // Sélectionner cette carte
                state.spots[3].selected_index = card_idx as i32;
                state.spots[3].cards[card_idx].is_selected = true;

                // CORRECTION: Avancer avec from_deal=true pour obtenir les bons résultats à la turn
                select_spot(game, &mut state, 4, false, true)?;

                println!("\n=== RÉSULTATS APRÈS LA TURN ===");
                // À ce stade, state.spots[4] est un nœud joueur OOP après la turn
                // avec les stratégies et EVs correctement calculés

                // Afficher les informations détaillées
                print_hand_details(game, 5);
            } else {
                return Err("Aucune carte disponible pour la turn!".to_string());
            }
        } else {
            return Err("Action Call non trouvée pour IP!".to_string());
        }
    } else {
        return Err("Action Bet non trouvée pour OOP!".to_string());
    }

    Ok(())
}
