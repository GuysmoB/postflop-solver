use crate::holes_to_strings;
use crate::Card;
use crate::PostFlopGame;
// Remove this line:
// use postflop_solver::action_tree::Action;
use postflop_solver::*;
// Remove this line too:
// use postflop_solver::Action; // The enum is likely re-exported at the root level
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
    let target_exploitability = 5.0;
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

    println!("\n=== DÉTAILS DES MAINS ===");
    // print_hand_details(&mut game, 5);
    explore_random_path(&mut game);
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
    println!("\n===== SIMULATION DU SCÉNARIO: OOP CHECK → IP BET =====");

    // S'assurer que nous commençons à la racine
    game.back_to_root();

    // Garder une trace du chemin parcouru
    let mut path = Vec::new();
    let mut node_count = 0;

    // Premier tour: OOP CHECK
    node_count += 1;
    println!("\n----- NŒUD #{} -----", node_count);

    // Vérifier que nous sommes avec le joueur OOP
    let player = game.current_player();
    if player != 0 {
        println!("Erreur: Le premier joueur n'est pas OOP!");
        return;
    }

    println!("Joueur actuel: OOP (0)");

    // Récupérer les actions disponibles
    let actions = game.available_actions();
    if actions.is_empty() {
        println!("Erreur: Aucune action disponible!");
        return;
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

    // Afficher les détails des mains AVANT de jouer une action
    println!("\n=== DÉTAILS DU NŒUD AVANT ACTION (OOP) ===");
    print_hand_details(game, 3); // Afficher 3 mains

    // Chercher l'action CHECK
    let check_idx = actions.iter().position(|a| {
        let action_str = format!("{:?}", a).to_uppercase();
        action_str.contains("CHECK")
    });

    if let Some(action_idx) = check_idx {
        println!("\nAction choisie: CHECK ({})", action_idx);
        path.push("CHECK".to_string());

        // Jouer l'action CHECK
        game.play(action_idx);
    } else {
        println!("Erreur: Action CHECK non disponible!");
        return;
    }

    // Deuxième tour: IP BET
    node_count += 1;
    println!("\n----- NŒUD #{} -----", node_count);

    // Vérifier que nous sommes avec le joueur IP
    let player = game.current_player();
    if player != 1 {
        println!("Erreur: Le deuxième joueur n'est pas IP!");
        return;
    }

    println!("Joueur actuel: IP (1)");

    // Récupérer les actions disponibles
    let actions = game.available_actions();
    if actions.is_empty() {
        println!("Erreur: Aucune action disponible!");
        return;
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

    // Afficher les détails des mains AVANT de jouer une action
    println!("\n=== DÉTAILS DU NŒUD AVANT ACTION (IP) ===");
    print_hand_details(game, 3); // Afficher 3 mains

    // Chercher l'action BET
    let bet_idx = actions.iter().position(|a| {
        let action_str = format!("{:?}", a).to_uppercase();
        action_str.contains("BET")
    });

    if let Some(action_idx) = bet_idx {
        println!("\nAction choisie: BET ({})", action_idx);
        path.push("BET".to_string());

        // Jouer l'action BET
        game.play(action_idx);

        // Afficher le résultat après BET
        println!("\n=== ÉTAT APRÈS LA SÉQUENCE CHECK → BET ===");
        println!("Chemin parcouru: {}", path.join(" → "));

        if !game.is_terminal_node() && !game.is_chance_node() {
            println!("\n=== DÉTAILS DU NŒUD ACTUEL ===");
            print_hand_details(game, 3);
        }
    } else {
        println!("Erreur: Action BET non disponible!");
        return;
    }
}
