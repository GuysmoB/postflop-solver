use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs::File;
use std::io::Write;
use std::{collections::HashMap, io::BufWriter};

use crate::{
    card_to_string_simple, deal, play, rank_to_char, round_to_decimal_places, save_spot_results,
    select_spot, suit_to_char, Card, GameState, PostFlopGame, Spot, SpotType,
};

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct ActionData {
    pub frequency: f32,
    pub ev: f32,
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct HandStrategy {
    #[serde(flatten)]
    pub actions: HashMap<String, ActionData>,
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct NodeStrategy {
    pub actions: Vec<String>,
    pub strategy: HashMap<String, HandStrategy>,
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct TreeNode {
    pub actions: Vec<String>,
    pub childrens: HashMap<String, Box<TreeNode>>,
    pub node_type: String,
    pub player: String,
    pub strategy: Option<NodeStrategy>,
    pub path: String,
}

pub fn save_exploration_results(game: &mut PostFlopGame, filename: &str) -> Result<(), String> {
    let tree = build_exploration_tree(game)?;
    let file = File::create(filename).map_err(|e| format!("Erreur création fichier: {}", e))?;
    let writer = BufWriter::new(file);
    // serde_json::to_writer(writer, &tree)
    //     .map_err(|e| format!("Erreur sérialisation JSON: {}", e))?;

    Ok(())
}

/// Construction récursive des nœuds de l'arbre d'exploration
fn build_node_recursive(
    game: &mut PostFlopGame,
    state: &mut GameState,
    path: &mut Vec<String>,
    flop_actions: &mut Vec<String>,
    turn_actions: &mut Vec<String>,
    river_actions: &mut Vec<String>,
    current_street: &mut &str,
) -> Result<TreeNode, String> {
    // println!("DEBUG - Current street: {}", current_street);
    // println!("DEBUG - Flop actions: {:?}", flop_actions);
    // println!("DEBUG - Turn actions: {:?}", turn_actions);
    // println!("DEBUG - River actions: {:?}", river_actions);

    if state.selected_chance_index > -1 {
        let chance_index = state.selected_chance_index as usize;
        if chance_index >= state.spots.len() {
            return Err(format!("Index de chance invalide: {}", chance_index));
        }

        let chance_spot = &state.spots[chance_index];
        if chance_spot.spot_type != SpotType::Chance {
            return Err(format!("Nœud non-chance à l'index {}", chance_index));
        }

        // Pour les nœuds chance, créer un nœud avec le type spécifique
        let mut node = TreeNode {
            node_type: "chance_node".to_string(),
            player: chance_spot.player.to_uppercase(),
            path: format_path_string(flop_actions, turn_actions, river_actions),
            ..Default::default()
        };

        // save_spot_results(game, "F:", "solver_data");

        let available_cards: Vec<usize> = chance_spot
            .cards
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.is_dead)
            .map(|(idx, _)| idx)
            .collect();

        if available_cards.is_empty() {
            return Err("Aucune carte disponible pour la distribution".to_string());
        }

        // Explorer chaque carte disponible
        let mut children = HashMap::new();

        for &card_idx in &available_cards {
            // Obtenir les informations de la carte
            let card_value = chance_spot.cards[card_idx].card as Card;
            let card_str = card_to_string_simple(card_value);

            // Créer des copies des vecteurs d'actions pour cette branche
            let mut new_flop_actions = flop_actions.clone();
            let mut new_turn_actions = turn_actions.clone();
            let mut new_river_actions = river_actions.clone();
            let mut new_current_street = *current_street;

            // Ajouter la carte au vecteur approprié selon la street
            if chance_spot.player == "turn" {
                new_current_street = "T";
                new_turn_actions.push(card_str.clone());
            } else if chance_spot.player == "river" {
                new_current_street = "R";
                new_river_actions.push(card_str.clone());
            }

            // Format pour le path traditionnel
            let player_capitalized = match chance_spot.player.as_str() {
                "turn" => "Turn",
                "river" => "River",
                other => other,
            };
            let card_path = format!("{}:{}", player_capitalized, card_str);
            path.push(card_path);

            // Sauvegarder l'état actuel
            let history_before = game.cloned_history();
            let mut new_state = state.clone();

            // Distribuer la carte
            deal(game, &mut new_state, card_idx)?;

            // Continuer l'exploration avec le nouvel état
            match build_node_recursive(
                game,
                &mut new_state,
                path,
                &mut new_flop_actions,
                &mut new_turn_actions,
                &mut new_river_actions,
                &mut new_current_street,
            ) {
                Ok(child_node) => {
                    children.insert(card_str, Box::new(child_node));
                }
                Err(e) => {
                    println!(
                        "Erreur lors de l'exploration de la carte {}: {}",
                        card_str, e
                    );
                }
            }

            // Restaurer l'état
            game.apply_history(&history_before);
            path.pop();
        }

        // Ajouter tous les enfants au nœud chance
        node.childrens = children;
        return Ok(node);
    }

    // Obtenir le spot actuel
    let current_spot_index = state.selected_spot_index as usize;
    let current_spot = match state.spots.get(current_spot_index) {
        Some(spot) => spot,
        None => return Err(format!("Spot à l'index {} non trouvé", current_spot_index)),
    };

    match current_spot.spot_type {
        // Nœud terminal
        SpotType::Terminal => Ok(TreeNode {
            node_type: "terminal_node".to_string(),
            player: "TERMINAL".to_string(),
            path: format_path_string(flop_actions, turn_actions, river_actions),
            ..Default::default()
        }),

        // Nœud joueur
        SpotType::Player => {
            // Créer le nœud d'action
            let mut node = TreeNode {
                node_type: "action_node".to_string(),
                player: current_spot.player.to_uppercase(),
                path: format_path_string(flop_actions, turn_actions, river_actions),
                ..Default::default()
            };

            // Ajouter les actions disponibles
            let mut action_names = Vec::new();
            for action in &current_spot.actions {
                let action_str = if action.amount != "0" {
                    format!("{} {}", action.name, action.amount)
                } else {
                    action.name.clone()
                };
                action_names.push(action_str);
            }
            node.actions = action_names.clone();

            let node_strategy = generate_node_strategy(game, state, &action_names);
            node.strategy = Some(node_strategy);

            // Explorer chaque action
            let mut children = HashMap::new();

            for (i, action) in current_spot.actions.iter().enumerate() {
                // Format pour l'action dans le nœud enfant
                let action_str = if action.amount != "0" {
                    format!("{} {}", action.name, action.amount)
                } else {
                    action.name.clone()
                };

                // Format pour le path
                let formatted_action = if action.amount != "0" {
                    format!("{}{}", action.name, action.amount)
                } else {
                    action.name.clone()
                };

                // MODIFICATION CRITIQUE: Cloner les vecteurs d'actions avant de les modifier
                let mut new_flop_actions = flop_actions.clone();
                let mut new_turn_actions = turn_actions.clone();
                let mut new_river_actions = river_actions.clone();

                // Ajouter l'action à la street appropriée
                match *current_street {
                    "F" => new_flop_actions.push(formatted_action.clone()),
                    "T" => new_turn_actions.push(formatted_action.clone()),
                    "R" => new_river_actions.push(formatted_action.clone()),
                    _ => new_flop_actions.push(formatted_action.clone()),
                }

                // Ajouter au path traditionnel
                path.push(action_str.clone());

                // Sauvegarder l'état avant de jouer l'action
                let history_before = game.cloned_history();
                let mut new_state = state.clone();

                // Jouer l'action
                play(game, &mut new_state, i)?;

                // MODIFICATION: Utiliser une copie de current_street pour chaque branche
                let mut new_current_street = *current_street;

                // Exploration récursive avec les nouveaux états
                match build_node_recursive(
                    game,
                    &mut new_state,
                    path,
                    &mut new_flop_actions,   // Utiliser les copies locales
                    &mut new_turn_actions,   // Utiliser les copies locales
                    &mut new_river_actions,  // Utiliser les copies locales
                    &mut new_current_street, // Utiliser la copie locale
                ) {
                    Ok(child_node) => {
                        children.insert(action_str, Box::new(child_node));
                    }
                    Err(e) => {
                        println!("Erreur lors de la construction d'un enfant: {}", e);
                    }
                }

                // Restaurer l'état
                game.apply_history(&history_before);
                path.pop();
            }

            node.childrens = children;
            Ok(node)
        }

        // Autres types de nœuds (racine)
        _ => {
            // Pour la racine, continuer l'exploration
            build_node_recursive(
                game,
                state,
                path,
                flop_actions,
                turn_actions,
                river_actions,
                current_street,
            )
        }
    }
}

// Fonction d'initialisation pour l'exploration d'arbre
fn build_exploration_tree(game: &mut PostFlopGame) -> Result<TreeNode, String> {
    // Initialiser l'état de jeu
    let mut state = GameState::new();
    let starting_pot = game.tree_config().starting_pot as f64;
    let effective_stack = game.tree_config().effective_stack as f64;

    // Créer le nœud racine
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
    select_spot(game, &mut state, 1, true, false)?;

    // Initialiser les vecteurs pour chaque street
    let mut flop_actions = Vec::new();
    let mut turn_actions = Vec::new();
    let mut river_actions = Vec::new();
    let mut current_street = "F"; // Commencer au flop

    // Commencer la construction récursive
    build_node_recursive(
        game,
        &mut state,
        &mut Vec::new(), // path initial vide
        &mut flop_actions,
        &mut turn_actions,
        &mut river_actions,
        &mut current_street,
    )
}

/// Fonction utilitaire pour formater correctement le path_string
fn format_path_string(
    flop_actions: &[String],
    turn_actions: &[String],
    river_actions: &[String],
) -> String {
    let mut parts = Vec::new();

    if flop_actions.is_empty() {
        parts.push("F:".to_string());
    } else {
        parts.push(format!("F:{}", flop_actions.join("-")));
    }

    // Ajouter les actions du turn s'il y en a
    // Le premier élément est la carte turn, les autres sont les actions
    if !turn_actions.is_empty() {
        if turn_actions.len() > 1 {
            // Premier élément est la carte
            let turn_card = &turn_actions[0];
            // Les éléments suivants sont les actions
            let actions = &turn_actions[1..];
            parts.push(format!("T:{}-{}", turn_card, actions.join("-")));
        } else {
            // S'il n'y a que la carte sans action
            parts.push(format!("T:{}", turn_actions[0]));
        }
    }

    // Ajouter les actions du river s'il y en a
    // Le premier élément est la carte river, les autres sont les actions
    if !river_actions.is_empty() {
        if river_actions.len() > 1 {
            // Premier élément est la carte
            let river_card = &river_actions[0];
            // Les éléments suivants sont les actions
            let actions = &river_actions[1..];
            parts.push(format!("R:{}-{}", river_card, actions.join("-")));
        } else {
            // S'il n'y a que la carte sans action
            parts.push(format!("R:{}", river_actions[0]));
        }
    }

    // Joindre toutes les parties avec des virgules
    parts.join(", ")
}

/// Génère les stratégies optimales pour un nœud de jeu
fn generate_node_strategy(
    game: &mut PostFlopGame,
    state: &GameState,
    action_names: &[String],
) -> NodeStrategy {
    let mut strategy = NodeStrategy {
        actions: action_names.to_vec(),
        strategy: HashMap::new(),
        // Supprimer le champ ev qui n'est pas dans votre structure
    };

    // Vérifions que nous ne sommes pas sur un nœud terminal ou chance
    // if game.is_terminal_node() || game.is_chance_node() {
    //     return strategy; // Pas de stratégie disponible
    // }

    // // Récupérer le joueur actuel
    // let current_spot_index = state.selected_spot_index as usize;
    // if let Some(current_spot) = state.spots.get(current_spot_index) {
    //     let player = if current_spot.player == "oop" { 0 } else { 1 };

    //     // S'assurer que les poids normalisés sont mis en cache
    //     game.cache_normalized_weights();

    //     // Obtenir la stratégie actuelle
    //     let solver_strategy = game.strategy();
    //     let num_actions = action_names.len();
    //     let num_hands = game.private_cards(player).len();

    //     // Récupérer les EVs pour chaque action
    //     let expected_values = game.expected_values_detail(player);

    //     // Obtenir la liste de toutes les combinaisons de mains possibles
    //     let hands = game.private_cards(player);

    //     // Pour chaque main, calculer la stratégie et les EVs
    //     for (hand_idx, hand_cards) in hands.iter().enumerate() {
    //         // Convertir la main en chaîne lisible
    //         let hand_str = format!(
    //             "{}{}{}{}",
    //             rank_to_char((hand_cards.0 / 4) as usize),
    //             suit_to_char((hand_cards.0 % 4) as usize),
    //             rank_to_char((hand_cards.1 / 4) as usize),
    //             suit_to_char((hand_cards.1 % 4) as usize)
    //         );

    //         // Créer un HandStrategy pour cette main
    //         let mut hand_strategy = HandStrategy {
    //             actions: HashMap::new(),
    //         };

    //         // Remplir les fréquences et EVs pour chaque action
    //         for action_idx in 0..num_actions {
    //             let action_name = &action_names[action_idx];

    //             // Index dans les tableaux de stratégie et EV
    //             let strategy_idx = action_idx * num_hands + hand_idx;
    //             let ev_idx = action_idx * num_hands + hand_idx;

    //             // Valeurs de fréquence et EV
    //             let frequency = if strategy_idx < solver_strategy.len() {
    //                 solver_strategy[strategy_idx]
    //             } else {
    //                 0.0
    //             };

    //             let ev_value = if ev_idx < expected_values.len() {
    //                 expected_values[ev_idx]
    //             } else {
    //                 0.0
    //             };

    //             // Créer ActionData avec les valeurs
    //             let action_data = ActionData {
    //                 frequency: round_to_decimal_places(frequency, 3),
    //                 ev: round_to_decimal_places(ev_value, 1),
    //             };

    //             // Ajouter à la structure HandStrategy
    //             hand_strategy
    //                 .actions
    //                 .insert(action_name.clone(), action_data);
    //         }

    //         // Ajouter cette main à la stratégie globale
    //         strategy.strategy.insert(hand_str, hand_strategy);
    //     }
    // }

    strategy
}
