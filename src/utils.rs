use crate::action_tree::Action as GameAction; // L'enum du jeu (Fold, Check, etc.)
use crate::holes_to_strings;
use crate::results::select_spot;
use crate::Card;
use crate::GameState;
use crate::PostFlopGame;
use crate::Spot;
use crate::SpotType;

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

pub struct SpecificResultData {
    pub current_player: String,
    pub num_actions: usize,
    pub is_empty: bool,
    pub eqr_base: [i32; 2],
    pub weights: Vec<Vec<f64>>,
    pub normalizer: Vec<Vec<f64>>,
    pub equity: Vec<Vec<f64>>,
    pub ev: Vec<Vec<f64>>,
    pub eqr: Vec<Vec<f64>>,
    pub strategy: Vec<f64>,
    pub action_ev: Vec<f64>,
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

pub fn print_hand_details(game: &mut PostFlopGame, max_hands: usize, results: &[f64]) {
    // S'assurer que nous sommes dans un nœud valide
    if game.is_terminal_node() || game.is_chance_node() {
        println!("Ce nœud ne contient pas de stratégie (terminal ou chance)");
        return;
    }

    game.cache_normalized_weights();

    // Récupérer le joueur actuel
    let player = game.current_player();
    let player_str = if player == 0 { "OOP" } else { "IP" };
    println!("\n=== DÉTAILS DU NŒUD ({}) ===", player_str);

    // Récupérer les données brutes selon la structure exacte
    let player_type = if player == 0 { "oop" } else { "ip" };
    let num_actions = game.available_actions().len();

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
    let pot_oop = results[offset];
    offset += 1;
    let pot_ip = results[offset];
    offset += 1;
    let is_empty_flag = results[offset] as usize;
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
                if strategy_offset + strat_idx < results.len() {
                    let strat_value = results[strategy_offset + strat_idx];
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
                if player_equity_offset + hand_idx >= results.len()
                    || player_ev_offset + hand_idx >= results.len()
                {
                    println!("Erreur: Indices hors limites pour la main {}", hand);
                    continue;
                }

                let equity = results[player_equity_offset + hand_idx] * 100.0;
                let ev = results[player_ev_offset + hand_idx];

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
                    if strategy_idx >= results.len() {
                        println!("    {{ /* Donnée de stratégie non disponible */ }},");
                        continue;
                    }

                    let frequency = results[strategy_idx] * 100.0;

                    // L'EV de cette action pour cette main (si disponible)
                    let action_ev = if action_ev_idx < results.len() {
                        results[action_ev_idx]
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

pub fn get_specific_result(
    game: &mut PostFlopGame,
    current_player: &str,
    num_actions: usize,
) -> Result<SpecificResultData, String> {
    // 1. Récupérer les résultats bruts via get_results (comme dans le frontend)
    let buffer = get_results(game);

    use std::fs::File;
    use std::io::Write;

    let mut file = match File::create("buffer_debug.json") {
        Ok(file) => file,
        Err(e) => return Err(format!("Erreur lors de la création du fichier: {}", e)),
    };

    // Écrire l'en-tête JSON
    writeln!(file, "{{").unwrap();

    // Écrire le contenu du buffer
    for (i, value) in buffer.iter().enumerate() {
        // Ajouter une virgule sauf pour la dernière ligne
        let separator = if i < buffer.len() - 1 { "," } else { "" };
        writeln!(file, "    \"{}\": {}{}", i, value, separator).unwrap();
    }

    // Fermer l'objet JSON
    writeln!(file, "}}").unwrap();

    // 2. Déterminer les tailles des ranges
    let oop_range_size = game.private_cards(0).len();
    let ip_range_size = game.private_cards(1).len();
    let length = [oop_range_size, ip_range_size];

    // 3. Parser le buffer comme dans le frontend
    let mut offset = 0;
    let mut weights: Vec<Vec<f64>> = vec![Vec::new(), Vec::new()];
    let mut normalizer: Vec<Vec<f64>> = vec![Vec::new(), Vec::new()];
    let mut equity: Vec<Vec<f64>> = vec![Vec::new(), Vec::new()];
    let mut ev: Vec<Vec<f64>> = vec![Vec::new(), Vec::new()];
    let mut eqr: Vec<Vec<f64>> = vec![Vec::new(), Vec::new()];
    let mut strategy: Vec<f64> = Vec::new();
    let mut action_ev: Vec<f64> = Vec::new();

    // Header: pot OOP, pot IP, is_empty_flag
    let eqr_base = [buffer[0] as i32, buffer[1] as i32];
    offset += 2;

    let is_empty_flag = buffer[offset] as usize;
    let is_empty = is_empty_flag == 3; // OOP et IP ranges sont toutes les deux vides
    offset += 1;

    // Weights
    for i in 0..length[0] {
        weights[0].push(buffer[offset + i]);
    }
    offset += length[0];

    for i in 0..length[1] {
        weights[1].push(buffer[offset + i]);
    }
    offset += length[1];

    if is_empty_flag > 0 {
        // Si vide, normalizer = weights
        normalizer = weights.clone();
    } else {
        // Normalizer weights
        for i in 0..length[0] {
            normalizer[0].push(buffer[offset + i]);
        }
        offset += length[0];

        for i in 0..length[1] {
            normalizer[1].push(buffer[offset + i]);
        }
        offset += length[1];

        // Equity
        for i in 0..length[0] {
            equity[0].push(buffer[offset + i]);
        }
        offset += length[0];

        for i in 0..length[1] {
            equity[1].push(buffer[offset + i]);
        }
        offset += length[1];

        // EV
        for i in 0..length[0] {
            ev[0].push(buffer[offset + i]);
        }
        offset += length[0];

        for i in 0..length[1] {
            ev[1].push(buffer[offset + i]);
        }
        offset += length[1];

        // EQR
        for i in 0..length[0] {
            eqr[0].push(buffer[offset + i]);
        }
        offset += length[0];

        for i in 0..length[1] {
            eqr[1].push(buffer[offset + i]);
        }
        offset += length[1];
    }

    // Strategy et action EV pour le joueur actuel
    if ["oop", "ip"].contains(&current_player) {
        let player_index = if current_player == "oop" { 0 } else { 1 };
        let range_size = length[player_index];

        // Strategy
        for i in 0..(num_actions * range_size) {
            if offset + i < buffer.len() {
                strategy.push(buffer[offset + i]);
            } else {
                strategy.push(0.0);
            }
        }
        offset += num_actions * range_size;

        // Action EV (si pas vide)
        if !is_empty {
            for i in 0..(num_actions * range_size) {
                if offset + i < buffer.len() {
                    action_ev.push(buffer[offset + i]);
                } else {
                    action_ev.push(0.0);
                }
            }
        }
    }

    // 4. Construire et retourner le résultat
    Ok(SpecificResultData {
        current_player: current_player.to_string(),
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
    })
}

/// Exécuter le scénario: OOP bet, IP call, puis turn
pub fn run_bet_call_turn_scenario(game: &mut PostFlopGame) -> Result<(), String> {
    // Créer l'état du jeu
    let mut state = GameState::new();

    // Initialiser avec la racine (flop)
    let starting_pot = game.tree_config().starting_pot as f64;
    let effective_stack = game.tree_config().effective_stack as f64;
    let board = game.current_board();

    println!("\n=== DÉMARRAGE DU SCÉNARIO ===");
    println!("Pot initial: {:.2} bb", starting_pot);
    println!("Stack effectif: {:.2} bb", effective_stack);
    println!(
        "Board: {}",
        board
            .iter()
            .map(|&c| card_to_string_simple(c))
            .collect::<Vec<_>>()
            .join(" ")
    );

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
        prev_player: None,
    };

    state.spots.push(root_spot);

    // Sélectionner le premier spot (initialise le jeu et affiche les actions disponibles)
    let results = select_spot(game, &mut state, 1, true, false)?;

    // Afficher les statistiques initiales (EV, équité)
    println!("\n=== STATISTIQUES INITIALES (FLOP) ===");
    display_simple_stats(game);

    println!("\n=== DETAILS ===");
    // print_hand_details(game, 1, &*results);

    // Afficher les actions disponibles
    // println!("\nActions disponibles pour OOP:");
    // for (i, action) in state.spots[1].actions.iter().enumerate() {
    //     println!(
    //         "  {}: {} {}",
    //         i,
    //         action.name,
    //         if action.amount != "0" {
    //             &action.amount
    //         } else {
    //             ""
    //         }
    //     );
    // }

    // // À ce stade, state.spots[1] contient un nœud joueur OOP avec des actions
    // println!("\n=== ÉTAPE 1: OOP BET ===");

    // // Trouver l'index de l'action "Bet" pour OOP
    // let bet_idx = state.spots[1].actions.iter().position(|a| a.name == "Bet");
    // if let Some(bet_idx) = bet_idx {
    //     // Afficher l'action sélectionnée
    //     let bet_action = &state.spots[1].actions[bet_idx];
    //     println!(
    //         "Action sélectionnée: {} {}",
    //         bet_action.name,
    //         if bet_action.amount != "0" {
    //             &bet_action.amount
    //         } else {
    //             ""
    //         }
    //     );

    //     // Sélectionner cette action
    //     state.spots[1].selected_index = bet_idx as i32;
    //     state.spots[1].actions[bet_idx].is_selected = true;

    //     // Avancer au nœud suivant (IP)
    //     select_spot(game, &mut state, 2, true, false)?;

    //     // Afficher les statistiques après le bet OOP
    //     println!("\n=== STATISTIQUES APRÈS BET OOP ===");
    //     display_simple_stats(game);

    //     // Afficher les actions disponibles pour IP
    //     println!("\nActions disponibles pour IP:");
    //     for (i, action) in state.spots[2].actions.iter().enumerate() {
    //         println!(
    //             "  {}: {} {}",
    //             i,
    //             action.name,
    //             if action.amount != "0" {
    //                 &action.amount
    //             } else {
    //                 ""
    //             }
    //         );
    //     }

    //     println!("\n=== ÉTAPE 2: IP CALL ===");

    //     // Trouver l'index de l'action "Call" pour IP
    //     let call_idx = state.spots[2].actions.iter().position(|a| a.name == "Call");
    //     if let Some(call_idx) = call_idx {
    //         // Afficher l'action sélectionnée
    //         let call_action = &state.spots[2].actions[call_idx];
    //         println!(
    //             "Action sélectionnée: {} {}",
    //             call_action.name,
    //             if call_action.amount != "0" {
    //                 &call_action.amount
    //             } else {
    //                 ""
    //             }
    //         );

    //         // Sélectionner cette action
    //         state.spots[2].selected_index = call_idx as i32;
    //         state.spots[2].actions[call_idx].is_selected = true;

    //         // Avancer au nœud suivant (nœud de chance pour la turn)
    //         select_spot(game, &mut state, 3, true, false)?;

    //         println!("\n=== ÉTAPE 3: TURN (NŒUD DE CHANCE) ===");

    //         // Afficher les statistiques avant la distribution de la turn
    //         println!("\n=== STATISTIQUES AVANT DISTRIBUTION DE LA TURN ===");
    //         println!("Pot actuel: {:.2} bb", state.spots[2].pot);
    //         println!("Stack restant: {:.2} bb", state.spots[2].stack);

    //         // Afficher quelques cartes turn disponibles
    //         println!("\nExemples de cartes turn disponibles:");
    //         let mut cards_shown = 0;
    //         for (i, card) in state.spots[3].cards.iter().enumerate() {
    //             if !card.is_dead && cards_shown < 5 {
    //                 println!("  {}: {}", i, card_to_string_simple(card.card as Card));
    //                 cards_shown += 1;
    //             }
    //         }

    //         // Compter le nombre total de cartes disponibles
    //         let available_cards_count = state.spots[3].cards.iter().filter(|c| !c.is_dead).count();
    //         if available_cards_count > 5 {
    //             println!(
    //                 "  ... et {} cartes supplémentaires",
    //                 available_cards_count - 5
    //             );
    //         }

    //         // À ce stade, state.spots[3] est un nœud de chance (turn)
    //         // et state.spots[4] sera un nœud joueur OOP après la turn

    //         // Pour simuler la sélection d'une carte turn, choisissons la première carte disponible
    //         if let Some(card_idx) = state.spots[3].cards.iter().position(|c| !c.is_dead) {
    //             // Afficher la carte sélectionnée
    //             let selected_card = state.spots[3].cards[card_idx].card as Card;
    //             println!(
    //                 "\nCarte turn sélectionnée: {}",
    //                 card_to_string_simple(selected_card)
    //             );

    //             // Sélectionner cette carte
    //             state.spots[3].selected_index = card_idx as i32;
    //             state.spots[3].cards[card_idx].is_selected = true;

    //             // CORRECTION: Avancer avec from_deal=true pour obtenir les bons résultats à la turn
    //             select_spot(game, &mut state, 4, false, true)?;

    //             println!("\n=== RÉSULTATS APRÈS LA TURN ===");

    //             // Afficher le board actuel
    //             let current_board = game.current_board();
    //             println!(
    //                 "Board actuel: {}",
    //                 current_board
    //                     .iter()
    //                     .map(|&c| card_to_string_simple(c))
    //                     .collect::<Vec<_>>()
    //                     .join(" ")
    //             );

    //             // Afficher les statistiques après la distribution de la turn
    //             println!("\n=== STATISTIQUES APRÈS DISTRIBUTION DE LA TURN ===");
    //             display_simple_stats(game);

    //             // À ce stade, state.spots[4] est un nœud joueur OOP après la turn
    //             // avec les stratégies et EVs correctement calculés
    //             if state.spots.len() > 4 {
    //                 println!("\nActions disponibles pour OOP après la turn:");
    //                 for (i, action) in state.spots[4].actions.iter().enumerate() {
    //                     println!(
    //                         "  {}: {} {}",
    //                         i,
    //                         action.name,
    //                         if action.amount != "0" {
    //                             &action.amount
    //                         } else {
    //                             ""
    //                         }
    //                     );
    //                 }
    //             }

    //             // Afficher les informations détaillées
    //             // print_hand_details(game, 1);
    //         } else {
    //             return Err("Aucune carte disponible pour la turn!".to_string());
    //         }
    //     } else {
    //         return Err("Action Call non trouvée pour IP!".to_string());
    //     }
    // } else {
    //     return Err("Action Bet non trouvée pour OOP!".to_string());
    // }

    Ok(())
}

/// Afficher des statistiques simples pour le nœud actuel
fn display_simple_stats(game: &mut PostFlopGame) {
    // S'assurer que nous avons des poids normalisés
    game.cache_normalized_weights();

    // Afficher le type de nœud
    if game.is_terminal_node() {
        println!("Type de nœud: Terminal");
    } else if game.is_chance_node() {
        println!("Type de nœud: Chance");
    } else {
        let player = if game.current_player() == 0 {
            "OOP"
        } else {
            "IP"
        };
        println!("Type de nœud: Joueur ({})", player);
    }

    // Afficher les montants de mises
    let total_bet = game.total_bet_amount();
    println!("Mise OOP: {:.2} bb", total_bet[0]);
    println!("Mise IP: {:.2} bb", total_bet[1]);

    // Pour les nœuds non-chance et non-terminaux, afficher les actions disponibles
    if !game.is_chance_node() && !game.is_terminal_node() {
        let actions = game.available_actions();
        println!("Actions disponibles: {}", actions.len());
        for (i, action) in actions.iter().enumerate() {
            println!("  {}: {:?}", i, action);
        }
    }

    // Calculer et afficher les équités moyennes
    // if !game.is_chance_node() {
    //     let oop_equity = calculate_average_equity(game, 0);
    //     let ip_equity = calculate_average_equity(game, 1);
    //     println!("Équité moyenne OOP: {:.2}%", oop_equity * 100.0);
    //     println!("Équité moyenne IP: {:.2}%", ip_equity * 100.0);
    // }
}

/// Calcule l'équité moyenne pour un joueur
fn calculate_average_equity(game: &PostFlopGame, player: usize) -> f64 {
    let equity = game.equity(player);
    let weights = game.normalized_weights(player);

    let mut total_equity = 0.0;
    let mut total_weight = 0.0;

    for (i, &eq) in equity.iter().enumerate() {
        let weight = weights[i];
        total_equity += eq as f64 * weight as f64;
        total_weight += weight as f64;
    }

    if total_weight > 0.0 {
        total_equity / total_weight
    } else {
        0.0
    }
}

pub fn actions_after(game: &mut PostFlopGame, append: &[i32]) -> String {
    if append.is_empty() {
        return get_current_actions_string(game);
    }

    // Utiliser cloned_history pour éviter l'emprunt
    let history = game.cloned_history();

    // Jouer chaque action valide (ignorer les valeurs négatives)
    for &action in append {
        if action >= 0 {
            game.play(action as usize);
        }
    }

    // Capturer le résultat
    let result = get_current_actions_string(game);

    // Restaurer l'état d'origine
    game.apply_history(&history);

    // Retourner le résultat
    result
}

pub fn get_current_actions_string(game: &PostFlopGame) -> String {
    if game.is_terminal_node() {
        "terminal".to_string()
    } else if game.is_chance_node() {
        "chance".to_string()
    } else {
        game.available_actions()
            .iter()
            .map(|action| match action {
                GameAction::Fold => "Fold:0".to_string(),
                GameAction::Check => "Check:0".to_string(),
                GameAction::Call => "Call:0".to_string(),
                GameAction::Bet(amount) => format!("Bet:{}", amount),
                GameAction::Raise(amount) => format!("Raise:{}", amount),
                GameAction::AllIn(amount) => format!("Allin:{}", amount),
                _ => unreachable!(),
            })
            .collect::<Vec<_>>()
            .join("/")
    }
}

// Fonction pour afficher un log complet de l'état du jeu
// pub fn log_game_state(game: &PostFlopGame, current_player: &str, num_actions: usize) {
//     println!("\n==== ÉTAT COMPLET DU JEU ====");
//     println!(
//         "État du nœud: terminal={}, chance={}",
//         game.is_terminal_node(),
//         game.is_chance_node()
//     );
//     println!("Joueur actuel: {}", current_player);
//     println!("Nombre d'actions: {}", num_actions);
//     println!("Cartes du board: {:?}", game.current_board());
//     println!("Pot de départ: {}", game.tree_config().starting_pot);
//     println!("Stack effectif: {}", game.tree_config().effective_stack);
//     println!(
//         "Montants misés: OOP={}, IP={}",
//         game.total_bet_amount()[0],
//         game.total_bet_amount()[1]
//     );

//     // Log des actions disponibles
//     if !game.is_terminal_node() && !game.is_chance_node() {
//         println!("Actions disponibles:");
//         for (i, action) in game.available_actions().iter().enumerate() {
//             println!("  {}: {:?}", i, action);
//         }
//     }

//     // Log des poids et de l'équité
//     println!("Poids OOP:");
//     let oop_weights = game.weights(0);
//     for i in 0..std::cmp::min(5, oop_weights.len()) {
//         print!("{:.4} ", oop_weights[i]);
//     }
//     println!("... ({} au total)", oop_weights.len());

//     println!("Poids IP:");
//     let ip_weights = game.weights(1);
//     for i in 0..std::cmp::min(5, ip_weights.len()) {
//         print!("{:.4} ", ip_weights[i]);
//     }
//     println!("... ({} au total)", ip_weights.len());

//     // Équité si disponible
//     if !game.is_terminal_node() && !game.is_chance_node() {
//         // Utiliser des appels sécurisés pour éviter les problèmes avec cache_normalized_weights

//         // Stratégie si nœud joueur
//         if current_player == "oop" || current_player == "ip" {
//             println!("Stratégie (5 premières valeurs):");
//             let strategy = game.strategy();
//             for i in 0..std::cmp::min(5, strategy.len()) {
//                 print!("{:.4} ", strategy[i]);
//             }
//             println!("... ({} au total)", strategy.len());

//             println!("Action EVs (5 premières valeurs):");
//             let action_evs = game.expected_values_detail(game.current_player());
//             for i in 0..std::cmp::min(5, action_evs.len()) {
//                 print!("{:.4} ", action_evs[i]);
//             }
//             println!("... ({} au total)", action_evs.len());
//         }
//     }
//     println!("==== FIN DU LOG DU JEU ====\n");
// }
