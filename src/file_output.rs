use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;

use crate::{
    card_to_string_simple, deal, play, select_spot, Card, GameState, PostFlopGame, Spot, SpotType,
};

/// Structures pour le format JSON des résultats d'exploration
#[derive(Serialize, Deserialize, Default, Debug)]
pub struct NodeStrategy {
    pub actions: Vec<String>,
    pub strategy: HashMap<String, Vec<f32>>,
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct TreeNode {
    pub actions: Vec<String>,
    pub childrens: HashMap<String, Box<TreeNode>>,
    pub node_type: String,
    pub player: String,
    pub strategy: Option<NodeStrategy>,
    pub path: Vec<String>,
    pub path_string: String,
}

/// Sauvegarde les résultats de l'exploration d'arbre dans un fichier JSON
pub fn save_exploration_results(game: &mut PostFlopGame, filename: &str) -> Result<(), String> {
    println!("\n=== SAUVEGARDE DES RÉSULTATS D'EXPLORATION EN JSON ===");

    // Construire la structure récursive des résultats
    let mut tree = build_exploration_tree(game)?;

    // Sérialiser en JSON avec formatage
    let json = serde_json::to_string_pretty(&tree)
        .map_err(|e| format!("Erreur de sérialisation JSON: {}", e))?;

    // Écrire dans un fichier
    let mut file = File::create(filename)
        .map_err(|e| format!("Erreur lors de la création du fichier: {}", e))?;

    file.write_all(json.as_bytes())
        .map_err(|e| format!("Erreur lors de l'écriture dans le fichier: {}", e))?;

    println!("Résultats sauvegardés dans {}", filename);
    Ok(())
}

/// Construction récursive des nœuds de l'arbre d'exploration
fn build_node_recursive(
    game: &mut PostFlopGame,
    state: &mut GameState,
    path: &mut Vec<String>,
    // Nouveaux paramètres pour tracker l'état par street
    flop_actions: &mut Vec<String>,
    turn_actions: &mut Vec<String>,
    river_actions: &mut Vec<String>,
    current_street: &mut &str,
) -> Result<TreeNode, String> {
    // Si c'est un nœud chance
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
            path: path.clone(),
            path_string: format_path_string(flop_actions, turn_actions, river_actions),
            ..Default::default()
        };

        // Sélectionner une carte
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

        let card_idx = available_cards[0];
        let card_value = chance_spot.cards[card_idx].card as Card;
        let card_str = card_to_string_simple(card_value);

        // Déterminer la street en fonction du type de chance
        if chance_spot.player == "turn" {
            *current_street = "T";
            turn_actions.push(card_str.to_string());
        } else if chance_spot.player == "river" {
            *current_street = "R";
            river_actions.push(card_str.to_string());
        }

        // Toujours garder le chemin traditionnel pour la compatibilité
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

        // Continuer l'exploration
        let child_node = build_node_recursive(
            game,
            &mut new_state,
            path,
            flop_actions,
            turn_actions,
            river_actions,
            current_street,
        )?;

        // Ajouter l'enfant à ce nœud
        node.childrens.insert(card_str, Box::new(child_node));

        // Restaurer l'état
        game.apply_history(&history_before);
        path.pop();

        // Retirer la carte du tracker si nécessaire
        if chance_spot.player == "turn" && !turn_actions.is_empty() {
            turn_actions.pop();
        } else if chance_spot.player == "river" && !river_actions.is_empty() {
            river_actions.pop();
        }

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
            path: path.clone(),
            path_string: format_path_string(flop_actions, turn_actions, river_actions),
            ..Default::default()
        }),

        // Nœud joueur
        SpotType::Player => {
            // Créer le nœud d'action
            let mut node = TreeNode {
                node_type: "action_node".to_string(),
                player: current_spot.player.to_uppercase(),
                path: path.clone(),
                path_string: format_path_string(flop_actions, turn_actions, river_actions),
                ..Default::default()
            };

            // Initialisation des actions et stratégies comme avant
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

            // Configuration de la stratégie comme avant
            let mut strategy = NodeStrategy {
                actions: action_names.clone(),
                strategy: HashMap::new(),
            };

            let sample_hands = ["AhAd", "KhKd", "QhQd", "JhJd", "ThTd"];
            for hand in &sample_hands {
                let mut rates = Vec::new();
                let n_actions = action_names.len();

                let mut rng = rand::thread_rng();
                let mut sum: f32 = 0.0;
                let mut values = Vec::new();

                for _ in 0..n_actions {
                    let val: f32 = rng.gen();
                    sum += val;
                    values.push(val);
                }

                for v in values {
                    rates.push(v / sum);
                }

                strategy.strategy.insert(hand.to_string(), rates);
            }

            node.strategy = Some(strategy);

            // Explorer chaque action
            let mut children = HashMap::new();

            for (i, action) in current_spot.actions.iter().enumerate() {
                let action_str = if action.amount != "0" {
                    format!("{} {}", action.name, action.amount)
                } else {
                    action.name.clone()
                };

                // Format pour le nouveau système de path
                let formatted_action = if action.amount != "0" {
                    format!("{}{}", action.name, action.amount) // Enlever l'espace entre l'action et le montant
                } else {
                    action.name.clone()
                };

                // Ajouter l'action à la street appropriée
                match *current_street {
                    "F" => flop_actions.push(formatted_action.clone()),
                    "T" => turn_actions.push(formatted_action.clone()),
                    "R" => river_actions.push(formatted_action.clone()),
                    _ => flop_actions.push(formatted_action.clone()), // Par défaut
                }

                // Garder aussi le chemin traditionnel
                path.push(if action.amount != "0" {
                    format!("{} {}", action.name, action.amount)
                } else {
                    action.name.clone()
                });

                // Sauvegarder l'état et jouer l'action
                let history_before = game.cloned_history();
                let mut new_state = state.clone();
                play(game, &mut new_state, i)?;

                // Exploration récursive
                match build_node_recursive(
                    game,
                    &mut new_state,
                    path,
                    flop_actions,
                    turn_actions,
                    river_actions,
                    current_street,
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

                // Retirer l'action du tracker de la street appropriée
                match *current_street {
                    "F" if !flop_actions.is_empty() => {
                        flop_actions.pop();
                    }
                    "T" if !turn_actions.is_empty() => {
                        turn_actions.pop();
                    }
                    "R" if !river_actions.is_empty() => {
                        river_actions.pop();
                    }
                    _ => {
                        if !flop_actions.is_empty() {
                            flop_actions.pop();
                        }
                    }
                }
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

/// Fonction utilitaire pour formater le path_string selon la nouvelle convention
fn format_path_string(
    flop_actions: &[String],
    turn_actions: &[String],
    river_actions: &[String],
) -> String {
    let mut parts = Vec::new();

    // Ajouter les actions du flop s'il y en a
    if !flop_actions.is_empty() {
        parts.push(format!("F:{}", flop_actions.join("-")));
    }

    // Ajouter les actions du turn s'il y en a
    if !turn_actions.is_empty() {
        parts.push(format!("T:{}", turn_actions.join("-")));
    }

    // Ajouter les actions du river s'il y en a
    if !river_actions.is_empty() {
        parts.push(format!("R:{}", river_actions.join("-")));
    }

    // Joindre toutes les parties avec des virgules
    parts.join(", ")
}

/// Construit récursivement l'arbre d'exploration à partir de l'état actuel du jeu
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

    // Initialiser les vecteurs pour chaque street et l'état actuel
    let mut flop_actions = Vec::new();
    let mut turn_actions = Vec::new();
    let mut river_actions = Vec::new();
    let mut current_street = "F"; // Commencer au flop

    // Construire l'arbre récursivement avec le nouveau système de path
    build_node_recursive(
        game,
        &mut state,
        &mut Vec::new(),
        &mut flop_actions,
        &mut turn_actions,
        &mut river_actions,
        &mut current_street,
    )
}
