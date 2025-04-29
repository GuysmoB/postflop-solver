// use crate::log_game_state;
use crate::utils::actions_after;
use crate::utils::get_current_actions_string;
use crate::utils::get_specific_result;
use crate::Card;
use crate::PostFlopGame;
use crate::SpecificResultData;

// Types pour représenter les spots (nœuds) dans l'arbre de jeu
#[derive(Debug, Clone, PartialEq)]
pub enum SpotType {
    Root,
    Player,
    Chance,
    Terminal,
}

pub struct SpotSelectionResult {
    pub selected_spot: Option<Spot>,
    pub selected_chance: Option<Spot>,
    pub current_board: Vec<Card>,
    pub results: Box<[f64]>,
    pub chance_reports: Option<Box<[f64]>>,
    pub total_bet_amount: [f64; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub index: usize,
    pub name: String,
    pub amount: String,
    pub is_selected: bool,
    pub rate: f64, // Taux pour les statistiques
}

#[derive(Debug, Clone)]
pub struct SpotCard {
    pub card: usize,
    pub is_selected: bool,
    pub is_dead: bool,
}

#[derive(Debug, Clone)]
pub struct Spot {
    pub spot_type: SpotType,
    pub index: usize,
    pub player: String,
    pub selected_index: i32,
    pub actions: Vec<Action>,
    pub cards: Vec<SpotCard>,
    pub prev_player: Option<String>, // Nouveau champ
    pub pot: f64,
    pub stack: f64,
    pub equity_oop: f64,
}

// Gestionnaire d'état global (similaire aux refs de Vue)
pub struct GameState {
    pub spots: Vec<Spot>,
    pub selected_spot_index: i32,
    pub selected_chance_index: i32,
    pub is_dealing: bool,
    pub results_empty: bool, // Indique si les résultats ont été calculés
    pub equity_oop: f64,     // L'équité du joueur OOP
    pub total_bet_amount_appended: [i32; 2], // Montants des mises [OOP, IP]
    pub can_chance_reports: bool, // Indique si les rapports de chance sont disponibles
    pub last_results: Option<Box<[f64]>>,
}

impl GameState {
    pub fn new() -> Self {
        Self {
            spots: Vec::new(),
            selected_spot_index: -1,
            selected_chance_index: -1,
            is_dealing: false,
            results_empty: true,
            equity_oop: 0.0,
            total_bet_amount_appended: [0, 0],
            can_chance_reports: false,
            last_results: None,
        }
    }
}

/// Function to navigate to a specific node in the game tree
/// Based on selectSpotFront from ResultNav.vue
pub fn select_spot(
    game: &mut PostFlopGame,
    state: &mut GameState,
    spot_index: usize,
    need_splice: bool,
    from_deal: bool,
) -> Result<SpecificResultData, String> {
    // Skip if already at the selected spot and not from a deal action
    if !need_splice
        && (spot_index == state.selected_spot_index as usize && !from_deal
            || spot_index == state.selected_chance_index as usize
            || (state.spots[spot_index].spot_type == SpotType::Chance
                && state.selected_chance_index != -1
                && state.spots[state.selected_chance_index as usize].selected_index == -1
                && spot_index > state.selected_chance_index as usize))
    {
        return Err("Spot déjà sélectionné".to_string());
    }

    // If spot_index is 0, select spot 1 instead
    if spot_index == 0 {
        return select_spot(game, state, 1, true, false);
    }

    // Store temporary values for indices to avoid unnecessary ref updates
    let mut selected_spot_index_tmp = state.selected_spot_index;
    let mut selected_chance_index_tmp = state.selected_chance_index;

    // Update selected indices based on spot type
    if !need_splice && state.spots[spot_index].spot_type == SpotType::Chance {
        selected_chance_index_tmp = spot_index as i32;
        if selected_spot_index_tmp < (spot_index + 1) as i32 {
            selected_spot_index_tmp = (spot_index + 1) as i32;
        }
    } else {
        selected_spot_index_tmp = spot_index as i32;
        if (spot_index as i32) <= selected_chance_index_tmp {
            selected_chance_index_tmp = -1;
        } else if selected_chance_index_tmp == -1 {
            // Find first chance node with no selection before spot_index
            for i in 0..spot_index {
                if state.spots[i].spot_type == SpotType::Chance
                    && state.spots[i].selected_index == -1
                {
                    selected_chance_index_tmp = i as i32;
                    break;
                }
            }
        }
    }

    // Determine end index for history application
    let end_index = if selected_chance_index_tmp == -1 {
        selected_spot_index_tmp as usize
    } else {
        selected_chance_index_tmp as usize
    };

    // Back to root and apply history
    game.back_to_root();

    // Build history array from spots (comme dans le code Vue)
    let mut history: Vec<usize> = Vec::new();
    for i in 1..end_index {
        if state.spots[i].selected_index != -1 {
            history.push(state.spots[i].selected_index as usize);
        }
    }

    // Apply history to game
    game.apply_history(&history);

    // Get current player type and number of actions
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

    // log_game_state(game, current_player, num_actions);
    // Obtenir les résultats au format identique à getSpecificResultsFront
    let results = get_specific_result(game, current_player, num_actions)?;

    // print results for debugging
    println!(
        "Results: current_player={}, num_actions={}, is_empty={}",
        results.current_player, results.num_actions, results.is_empty
    );

    if !results.action_ev.is_empty() {
        let display_count = std::cmp::min(5, results.action_ev.len());
        println!(
            "Action EV (5 premières valeurs): [{}]",
            results.action_ev[..display_count]
                .iter()
                .map(|&v| format!("{:.4}", v))
                .collect::<Vec<String>>()
                .join(", ")
        );
    }

    // Check if results are empty
    state.results_empty = results.is_empty;

    // Construire l'append array comme dans le frontend
    let mut append_array: Vec<i32> = Vec::new();

    if selected_chance_index_tmp != -1 {
        for i in selected_chance_index_tmp as usize..selected_spot_index_tmp as usize {
            append_array.push(state.spots[i].selected_index);
        }
    }

    // Convertir en Rust-friendly array
    let append: Vec<usize> = append_array
        .iter()
        .map(|&x| if x < 0 { 0 } else { x as usize })
        .collect();

    // Obtenir les actions après le skip des chances
    let next_actions_str = actions_after(game, &append_array);

    // Vérifier si on peut avoir des chance reports
    let can_chance_reports = selected_chance_index_tmp != -1
        && state.spots[(selected_chance_index_tmp + 3) as usize..selected_spot_index_tmp as usize]
            .iter()
            .all(|spot| spot.spot_type != SpotType::Chance)
        && next_actions_str != "chance";

    state.can_chance_reports = can_chance_reports;

    // Obtenir les chance reports si possible
    // let mut chance_reports = None;
    if can_chance_reports {
        // Ici, on implémenterait l'équivalent de getChanceReports
        // Pour l'instant on laisse à None
        println!("Chance reports disponibles mais non implémentés");
    }

    // Update total bet amount
    state.total_bet_amount_appended = game.total_bet_amount();

    // Update spots if needed (splice)
    if need_splice {
        // Remove all spots after the current one
        state.spots.truncate(spot_index);

        if game.is_terminal_node() || next_actions_str == "terminal" {
            // Create a terminal spot
            splice_spots_terminal(game, state, spot_index)?;
        } else if game.is_chance_node() || next_actions_str == "chance" {
            // Create a chance spot
            splice_spots_chance(game, state, spot_index)?;
            // Increment selected spot index after creating a chance node
            selected_spot_index_tmp += 1;
        } else {
            // Create a player spot
            splice_spots_player(state, spot_index, next_actions_str)?;
        }
    }

    // Update action rates for selected player spot
    if let Some(spot) = state.spots.get_mut(selected_spot_index_tmp as usize) {
        if spot.spot_type == SpotType::Player && selected_chance_index_tmp == -1 {
            let player_index = if spot.player == "oop" { 0 } else { 1 };

            if !state.results_empty {
                update_action_rates(spot, game, player_index);
            }
        }
    }

    // Update indices after all processing
    state.selected_spot_index = selected_spot_index_tmp;
    state.selected_chance_index = selected_chance_index_tmp;
    state.is_dealing = false;

    // Handle special from_deal processing
    if from_deal {
        process_from_deal(game, state)?;
    }

    // Construire un objet SpotSelectionResult avec toutes les informations nécessaires
    Ok(results)
}

/// Helper function to update action rates for a player spot
fn update_action_rates(spot: &mut Spot, game: &PostFlopGame, player_index: usize) {
    let strategy = game.strategy();
    let weights = game.normalized_weights(player_index);
    let num_hands = weights.len();

    for (i, action) in spot.actions.iter_mut().enumerate() {
        let mut total_weight = 0.0;
        let mut total_freq = 0.0;

        for hand_idx in 0..num_hands {
            let weight = weights[hand_idx];
            total_weight += weight as f64;

            let strat_idx = hand_idx + i * num_hands;
            if strat_idx < strategy.len() {
                total_freq += strategy[strat_idx] as f64 * weight as f64;
            }
        }

        action.rate = if total_weight > 0.0 {
            (total_freq / total_weight) as f64
        } else {
            0.0
        };

        println!("Action: {}, Rate: {}", action.name, action.rate);
    }
}

/// Process special handling for actions coming from deal()
fn process_from_deal(game: &mut PostFlopGame, state: &mut GameState) -> Result<(), String> {
    // This is where we would update river dead cards and terminal equity
    // when we've just performed a deal action on turn

    // Find river spot after selected chance index (if any)
    if state.selected_chance_index > 0 {
        let start_idx = (state.selected_chance_index as usize) + 3;
        let mut river_idx = -1;

        for i in start_idx..state.spots.len() {
            if state.spots[i].spot_type == SpotType::Chance {
                river_idx = i as i32;
                break;
            }
        }

        // Update river spot's dead cards
        if river_idx >= 0 {
            // First, collect history up to river spot
            let mut history = Vec::new();

            // Collect all selected indices first to avoid mutable borrow issues
            let selected_indices: Vec<(usize, SpotType, i32)> = (1..river_idx as usize)
                .filter_map(|i| {
                    let spot = &state.spots[i];
                    if spot.selected_index >= 0 {
                        Some((i, spot.spot_type.clone(), spot.selected_index))
                    } else {
                        None
                    }
                })
                .collect();

            // Now process the collected indices
            for (_, spot_type, selected_index) in selected_indices {
                match spot_type {
                    SpotType::Chance => {
                        // For chance spots, just add the selected index
                        history.push(selected_index as usize);
                    }
                    SpotType::Player => {
                        // For player spots, add the selected action index
                        history.push(selected_index as usize);
                    }
                    _ => {}
                }
            }

            // Back to root and apply history
            game.back_to_root();
            game.apply_history(&history);

            // Get possible cards and update river spot
            let possible_cards = game.possible_cards();
            let river_spot = &mut state.spots[river_idx as usize];

            for i in 0..52 {
                let is_dead = (possible_cards & (1u64 << i)) == 0;
                river_spot.cards[i].is_dead = is_dead;

                // If the selected card is now dead, deselect it
                if river_spot.selected_index as usize == i && is_dead {
                    river_spot.cards[i].is_selected = false;
                    river_spot.selected_index = -1;
                }
            }
        }

        // Check if the last spot is a terminal spot with non-fold equity
        // First collect the required information without holding any borrows
        let spots_len = state.spots.len();
        let last_spot_is_terminal = if let Some(last) = state.spots.last() {
            last.spot_type == SpotType::Terminal && last.equity_oop != 0.0 && last.equity_oop != 1.0
        } else {
            false
        };

        // If we need to update the terminal equity
        if last_spot_is_terminal {
            // Collect all selected indices first without borrowing state.spots mutably
            let selected_indices: Vec<i32> = (1..spots_len - 1)
                .filter_map(|i| {
                    let spot = &state.spots[i];
                    if spot.selected_index >= 0 {
                        Some(spot.selected_index)
                    } else {
                        None
                    }
                })
                .collect();

            // Now build the history from collected indices
            let history: Vec<usize> = selected_indices
                .into_iter()
                .map(|idx| idx as usize)
                .collect();

            // Back to root and apply history
            game.back_to_root();
            game.apply_history(&history);

            // Get results and calculate equity
            game.cache_normalized_weights();

            // Now we can safely mutably borrow the last spot
            if let Some(last_spot) = state.spots.last_mut() {
                if !state.results_empty {
                    let equity = calculate_average_equity(game, 0);
                    last_spot.equity_oop = equity;
                } else {
                    last_spot.equity_oop = -1.0;
                }
            }
        }
    }

    // Reset selected chance index
    state.selected_chance_index = -1;

    Ok(())
}

/// Calculate average equity for a player
fn calculate_average_equity(game: &PostFlopGame, player_index: usize) -> f64 {
    let equity = game.equity(player_index);
    let weights = game.normalized_weights(player_index);

    let mut total_equity = 0.0;
    let mut total_weight = 0.0;

    for i in 0..weights.len() {
        let weight = weights[i] as f64;
        total_equity += equity[i] as f64 * weight;
        total_weight += weight;
    }

    if total_weight > 0.0 {
        total_equity / total_weight
    } else {
        0.0
    }
}

/// Fonction pour mettre à jour les spots avec un nœud terminal
/// Reproduction fidèle de spliceSpotsTerminal dans ResultNav.vue
fn splice_spots_terminal(
    game: &mut PostFlopGame,
    state: &mut GameState,
    spot_index: usize,
) -> Result<(), String> {
    // Récupérer le spot précédent et son action sélectionnée
    let prev_spot = &state.spots[spot_index - 1];
    let prev_action = &prev_spot.actions[prev_spot.selected_index as usize];

    // Vérifier si un noeud de chance est sauté
    let chance_index = state.selected_chance_index;
    let chance_skipped =
        chance_index != -1 && state.spots[chance_index as usize].selected_index == -1;

    // Déterminer l'équité OOP en fonction de l'action précédente
    let equity_oop = if prev_action.name == "Fold" {
        // Si c'est un fold, l'équité dépend du joueur qui a foldé
        if prev_spot.player == "oop" {
            0.0
        } else {
            1.0
        }
    } else if chance_skipped || state.results_empty {
        // Si chance skippée ou résultats vides, équité indéterminée
        -1.0
    } else {
        // Sinon calculer l'équité moyenne (équivalent à average(results.equity[0], results.normalizer[0]))
        state.equity_oop
    };

    // Calculer le pot final (somme des mises des deux joueurs)
    let bet_sum = state.total_bet_amount_appended[0] + state.total_bet_amount_appended[1];
    let final_pot = game.tree_config().starting_pot as f64 + bet_sum as f64;

    // Créer le nouveau spot terminal
    let new_terminal_spot = Spot {
        spot_type: SpotType::Terminal,
        index: spot_index,
        player: "end".to_string(),
        selected_index: -1,
        cards: Vec::new(),
        actions: Vec::new(),
        pot: final_pot,
        stack: 0.0, // Le stack n'est pas utilisé dans les spots terminaux
        equity_oop,
        prev_player: Some(prev_spot.player.clone()),
    };

    // Remplacer tous les spots à partir de l'index actuel
    state.spots.truncate(spot_index);
    state.spots.push(new_terminal_spot);

    Ok(())
}

fn splice_spots_chance(
    game: &mut PostFlopGame,
    state: &mut GameState,
    spot_index: usize,
) -> Result<(), String> {
    // Récupérer le spot précédent (comme prevSpot dans le frontend)
    let prev_spot = &state.spots[spot_index - 1];

    // Chercher un spot de turn existant (comme turnSpot dans le frontend)
    let turn_spot = state
        .spots
        .iter()
        .take(spot_index)
        .find(|spot| spot.player == "turn");

    // Préparer le tableau pour append (comme appendArray dans le frontend)
    let mut append_array = Vec::new();
    if state.selected_chance_index != -1 {
        // Ajouter les indices sélectionnés entre le nœud de chance sélectionné et l'index actuel
        for i in state.selected_chance_index as usize..spot_index {
            append_array.push(state.spots[i].selected_index);
        }
    }

    // Déterminer les cartes possibles (comme possibleCards dans le frontend)
    let mut possible_cards = 0u64;
    // Si nous n'avons pas de turn, ou si le turn n'a pas de carte sélectionnée, on peut calculer
    if !(turn_spot.is_some()
        && turn_spot.unwrap().spot_type == SpotType::Chance
        && turn_spot.unwrap().selected_index == -1)
    {
        possible_cards = game.possible_cards();
    }

    // Ajouter -1 au append (comme dans le frontend)
    append_array.push(-1);

    // Obtenir les actions disponibles après ce append (comme nextActionsStr)
    let next_actions_str = actions_after(game, &append_array);
    let next_actions: Vec<&str> = next_actions_str.split('/').collect();

    // Mettre à jour canChanceReports (comme dans le frontend)
    let can_chance_reports = state.selected_chance_index == -1;

    // Si nous avons besoin de rapports de chance et que selectedChanceIndex est -1
    if can_chance_reports {
        // Obtenir les rapports de chance (équivalent à getChanceReports)
        println!(
            "Obtention des rapports de chance pour {} actions",
            next_actions.len()
        );
    }

    // Modifier le tableau des spots par splice (comme dans le frontend)
    // Créer le nouveau spot de chance (turn ou river)
    let new_chance_spot = Spot {
        spot_type: SpotType::Chance,
        index: spot_index,
        player: if turn_spot.is_some() {
            "river".to_string()
        } else {
            "turn".to_string()
        },
        selected_index: -1,
        cards: (0..52)
            .map(|i| SpotCard {
                card: i,
                is_selected: false,
                is_dead: (possible_cards & (1u64 << i)) == 0,
            })
            .collect(),
        actions: Vec::new(),
        pot: game.tree_config().starting_pot as f64 + 2.0 * game.total_bet_amount()[0] as f64,
        stack: game.tree_config().effective_stack as f64 - game.total_bet_amount()[0] as f64,
        equity_oop: 0.0,
        prev_player: Some(prev_spot.player.clone()),
    };

    // Créer le spot de joueur (toujours OOP après un nœud de chance)
    let mut player_actions = Vec::new();
    for (i, action) in next_actions.iter().enumerate() {
        let parts: Vec<&str> = action.split(':').collect();
        let name = parts[0].to_string();
        let amount = if parts.len() > 1 {
            parts[1].to_string()
        } else {
            "0".to_string()
        };

        player_actions.push(Action {
            index: i,
            name,
            amount,
            is_selected: false,
            rate: -1.0,
        });
    }

    let new_player_spot = Spot {
        spot_type: SpotType::Player,
        index: spot_index + 1,
        player: "oop".to_string(), // Toujours OOP qui joue après un nœud de chance
        selected_index: -1,
        cards: Vec::new(),
        actions: player_actions,
        pot: new_chance_spot.pot,
        stack: new_chance_spot.stack,
        equity_oop: 0.0,
        prev_player: Some(new_chance_spot.player.clone()), // Correction: ajout du champ manquant
    };

    // Faire le splice comme dans le frontend
    state.spots.truncate(spot_index);
    state.spots.push(new_chance_spot);
    state.spots.push(new_player_spot);

    // Incrémenter selectedSpotIndexTmp comme dans le frontend
    if state.selected_spot_index as usize == spot_index {
        state.selected_spot_index += 1;
    }

    // Mettre à jour selectedChanceIndexTmp si nécessaire
    if state.selected_chance_index == -1 {
        state.selected_chance_index = spot_index as i32;
    }

    Ok(())
}

/// Fonction pour mettre à jour les spots avec un nœud de joueur
/// Reproduction fidèle de spliceSpotsPlayer dans ResultNav.vue
fn splice_spots_player(
    state: &mut GameState,
    spot_index: usize,
    actions_str: String,
) -> Result<(), String> {
    // Récupérer le spot précédent
    let prev_spot = &state.spots[spot_index - 1];

    // Déterminer le joueur actuel (oop ou ip) en fonction du joueur précédent
    let player = if prev_spot.player == "oop" {
        "ip"
    } else {
        "oop"
    };

    // Analyser les actions à partir de la chaîne actions_str
    let actions: Vec<&str> = actions_str.split('/').collect();

    // Créer les actions pour le nouveau spot
    let player_actions: Vec<Action> = actions
        .iter()
        .enumerate()
        .map(|(i, action)| {
            let parts: Vec<&str> = action.split(':').collect();
            let name = parts[0].to_string();
            let amount = if parts.len() > 1 {
                parts[1].to_string()
            } else {
                "0".to_string()
            };

            Action {
                index: i,
                name,
                amount,
                is_selected: false,
                rate: -1.0,
            }
        })
        .collect();

    // Créer le nouveau spot de joueur
    let new_player_spot = Spot {
        spot_type: SpotType::Player,
        index: spot_index,
        player: player.to_string(),
        selected_index: -1,
        cards: Vec::new(),
        actions: player_actions,
        pot: prev_spot.pot,     // Utiliser le même pot que le spot précédent
        stack: prev_spot.stack, // Utiliser le même stack que le spot précédent
        equity_oop: 0.0,
        prev_player: Some(prev_spot.player.clone()),
    };

    // Remplacer tous les spots à partir de l'index actuel (comme dans le frontend)
    state.spots.truncate(spot_index);
    state.spots.push(new_player_spot);

    Ok(())
}

/// Fonction pour jouer une action sélectionnée (équivalent à play dans ResultNav.vue)
pub fn play(
    game: &mut PostFlopGame,
    state: &mut GameState,
    spot_index: usize,
    action_index: usize,
) -> Result<(), String> {
    // Récupérer le spot actuel
    let spot = match state.spots.get_mut(spot_index) {
        Some(s) => s,
        None => return Err(format!("Spot à l'index {} non trouvé", spot_index)),
    };

    // Vérifier que c'est bien un nœud de joueur
    if spot.spot_type != SpotType::Player {
        return Err(format!(
            "Le spot à l'index {} n'est pas un nœud joueur",
            spot_index
        ));
    }

    // Si une action est déjà sélectionnée, la désélectionner
    if spot.selected_index != -1 {
        if let Some(action) = spot.actions.get_mut(spot.selected_index as usize) {
            action.is_selected = false;
        }
    }

    // Sélectionner la nouvelle action
    if let Some(action) = spot.actions.get_mut(action_index) {
        action.is_selected = true;
    } else {
        return Err(format!("Action à l'index {} non trouvée", action_index));
    }

    // Mettre à jour l'index sélectionné
    spot.selected_index = action_index as i32;

    // Naviguer au spot suivant avec needSplice=true
    select_spot(game, state, spot_index + 1, true, false).map(|_| ())
}
