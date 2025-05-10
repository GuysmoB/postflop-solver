use crate::holes_to_strings;
use crate::utils::*;
use crate::weighted_average;
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

#[derive(Clone)]
pub struct GameState {
    pub spots: Vec<Spot>,
    pub selected_spot_index: i32,
    pub selected_chance_index: i32,
    pub is_dealing: bool,
    pub results_empty: bool, // Indique si les résultats ont été calculés
    pub equity_oop: f64,     // L'équité du joueur OOP
    pub total_bet_amount: Vec<u32>,
    pub total_bet_amount_appended: Vec<u32>, // Montants des mises [OOP, IP]
    pub can_chance_reports: bool,            // Indique si les rapports de chance sont disponibles
    pub results: SpecificResultData,
    pub chance_reports: Option<SpecificChanceReportData>,
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
            total_bet_amount: Vec::new(),
            total_bet_amount_appended: Vec::new(),
            can_chance_reports: false,
            results: SpecificResultData::default(),
            chance_reports: None,
        }
    }

    pub fn log_spot(&self, index: usize) {
        if let Some(spot) = self.spots.get(index) {
            println!(
                "Spot #{} - Type: {:?}, Player: {}, Index: {}",
                index, spot.spot_type, spot.player, spot.index
            );
            println!("  Selected index: {}", spot.selected_index);

            if !spot.actions.is_empty() {
                println!("  Actions:");
                for (j, action) in spot.actions.iter().enumerate() {
                    println!(
                        "    {}: {} {} (selected: {}, rate: {:.2})",
                        j, action.name, action.amount, action.is_selected, action.rate
                    );
                }
            }

            if spot.spot_type == SpotType::Chance {
                let selected_cards: Vec<_> = spot
                    .cards
                    .iter()
                    .filter(|c| c.is_selected)
                    .map(|c| c.card)
                    .collect();
                let dead_cards_count = spot.cards.iter().filter(|c| c.is_dead).count();
                println!(
                    "  Cards: {} cards, {} dead, Selected: {:?}",
                    spot.cards.len(),
                    dead_cards_count,
                    selected_cards
                );
            }

            println!(
                "  Pot: {:.2}, Stack: {:.2}, Equity OOP: {:.2}",
                spot.pot, spot.stack, spot.equity_oop
            );
            println!("  Previous player: {:?}", spot.prev_player);
        } else {
            println!("Spot à l'index {} non trouvé", index);
        }
    }

    pub fn log_spots(&self, prefix: &str) {
        println!("\n=== {} - SPOTS STATE ===", prefix);
        println!("Total spots: {}", self.spots.len());
        println!("Selected spot index: {}", self.selected_spot_index);
        println!("Selected chance index: {}", self.selected_chance_index);

        for (i, spot) in self.spots.iter().enumerate() {
            println!(
                "Spot #{} - Type: {:?}, Player: {}, Index: {}",
                i, spot.spot_type, spot.player, spot.index
            );
            println!("  Selected index: {}", spot.selected_index);

            if !spot.actions.is_empty() {
                println!("  Actions:");
                for (j, action) in spot.actions.iter().enumerate() {
                    println!(
                        "    {}: {} {} (selected: {}, rate: {:.2})",
                        j, action.name, action.amount, action.is_selected, action.rate
                    );
                }
            }

            if spot.spot_type == SpotType::Chance {
                let selected_cards: Vec<_> = spot
                    .cards
                    .iter()
                    .filter(|c| c.is_selected)
                    .map(|c| c.card)
                    .collect();
                let dead_cards_count = spot.cards.iter().filter(|c| c.is_dead).count();
                println!(
                    "  Cards: {} cards, {} dead, Selected: {:?}",
                    spot.cards.len(),
                    dead_cards_count,
                    selected_cards
                );
            }

            println!(
                "  Pot: {:.2}, Stack: {:.2}, Equity OOP: {:.2}",
                spot.pot, spot.stack, spot.equity_oop
            );
            println!("  Previous player: {:?}", spot.prev_player);
        }

        println!("===============================\n");
    }

    pub fn log_state(&self, prefix: &str) {
        println!("\n=== {} - GAME STATE ===", prefix);
        println!("Selected spot index: {}", self.selected_spot_index);
        println!("Selected chance index: {}", self.selected_chance_index);
        println!("Is dealing: {}", self.is_dealing);
        println!("Results empty: {}", self.results_empty);
        println!("Equity OOP: {:.4}", self.equity_oop);
        println!(
            "Total bet amount: OOP={}, IP={}",
            self.total_bet_amount_appended[0], self.total_bet_amount_appended[1]
        );
        println!("Can chance reports: {}", self.can_chance_reports);
        // println!("Has last results: {}", self.last_results.is_some());
        // self.log_spots(prefix);
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
    // println!(
    //     "select_spot() - spot_index: {}, need_splice: {}, from_deal: {}",
    //     spot_index, need_splice, from_deal
    // );

    if !need_splice
        && (spot_index == state.selected_spot_index as usize && !from_deal
            || spot_index == state.selected_chance_index as usize
            || (state.spots[spot_index].spot_type == SpotType::Chance
                && state.selected_chance_index != -1
                && state.spots[state.selected_chance_index as usize].selected_index == -1
                && spot_index > state.selected_chance_index as usize))
    {
        println!("select_spot() - No need to select a new spot");
        return Ok(state.results.clone());
    }

    if spot_index == 0 {
        return select_spot(game, state, 1, true, false);
    }

    // Store temporary values for indices to avoid unnecessary ref updates
    let mut selected_spot_index_tmp = state.selected_spot_index;
    let mut selected_chance_index_tmp = state.selected_chance_index;

    if from_deal {
        let new_selected_chance_index = process_from_deal(game, state);
        selected_chance_index_tmp = new_selected_chance_index;
    }

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

    let end_index = if selected_chance_index_tmp == -1 {
        selected_spot_index_tmp as usize
    } else {
        selected_chance_index_tmp as usize
    };

    let mut history: Vec<usize> = Vec::new();
    for i in 1..end_index {
        if state.spots[i].selected_index != -1 {
            history.push(state.spots[i].selected_index as usize);
        }
    }

    game.apply_history(&history);

    let current_player = current_player_str(game);
    let num_actions = if ["terminal", "chance"].contains(&current_player) {
        0
    } else {
        game.available_actions().len()
    };

    let results = get_specific_result(game, current_player, num_actions)?;
    state.results = results.clone();
    state.results_empty = results.is_empty;

    // Extract flop actions from the history
    let mut flop_actions = Vec::new();
    for i in 1..end_index {
        if state.spots[i].selected_index != -1
            && state.spots[i].spot_type == SpotType::Player
            && state.spots[i].player != "turn"
            && state.spots[i].player != "river"
        {
            if let Some(action) = state.spots[i]
                .actions
                .get(state.spots[i].selected_index as usize)
            {
                let action_str = if action.amount != "0" {
                    format!("{}{}", action.name, action.amount)
                } else {
                    action.name.clone()
                };
                flop_actions.push(action_str);
            }
        }
    }

    // Save flop action results to file with the real action history
    if let Err(e) = save_flop_results(game, Some(&flop_actions)) {
        println!("Warning: Failed to save flop results: {}", e);
    }

    let mut append_array: Vec<i32> = Vec::new();
    if selected_chance_index_tmp != -1 {
        for i in selected_chance_index_tmp as usize..selected_spot_index_tmp as usize {
            append_array.push(state.spots[i].selected_index);
        }
    }

    let append: Vec<usize> = append_array
        .iter()
        .map(|&x| if x < 0 { 0 } else { x as usize })
        .collect();

    let next_actions_str = actions_after(game, &append);
    let can_chance_reports = selected_chance_index_tmp != -1
        && state.spots[(selected_chance_index_tmp + 3) as usize..selected_spot_index_tmp as usize]
            .iter()
            .all(|spot| spot.spot_type != SpotType::Chance)
        && next_actions_str != "chance";

    state.can_chance_reports = can_chance_reports;

    if can_chance_reports {
        let (player, num_actions) = if next_actions_str == "terminal" {
            ("terminal", 0)
        } else {
            let player = if append_array.len() % 2 == 1 {
                "oop"
            } else {
                "ip"
            };
            let num_actions = next_actions_str.split('/').count();
            (player, num_actions)
        };

        println!(
            "Obtention des rapports de chance: joueur={}, actions={}",
            player, num_actions
        );

        match get_specific_chance_reports(game, &append, player, num_actions) {
            Ok(reports) => {
                // println!(
                //     "Rapports de chance obtenus avec succès ({} valeurs)",
                //     reports.strategy.len()
                // );
                state.chance_reports = Some(reports);
            }
            Err(e) => {
                println!("Erreur lors de l'obtention des rapports de chance: {}", e);
                state.chance_reports = None;
            }
        }
    } else {
        state.chance_reports = None;
    }

    let empty_append: Vec<usize> = Vec::new();
    state.total_bet_amount = total_bet_amount(game, &empty_append);
    state.total_bet_amount_appended = total_bet_amount(game, &append);

    // Update spots if needed (splice)
    if need_splice {
        state.spots.truncate(spot_index);

        if next_actions_str == "terminal" {
            splice_spots_terminal(game, state, spot_index)?;
        } else if next_actions_str == "chance" {
            let (new_selected_chance_index, new_selected_spot_index) =
                splice_spots_chance(game, state, spot_index)?;
            selected_chance_index_tmp = new_selected_chance_index;
            selected_spot_index_tmp = new_selected_spot_index
        } else {
            splice_spots_player(state, spot_index, next_actions_str)?;
        }
    }

    if let Some(spot) = state.spots.get_mut(selected_spot_index_tmp as usize) {
        if spot.spot_type == SpotType::Player && selected_chance_index_tmp == -1 {
            let player_index = if spot.player == "oop" { 0 } else { 1 };

            if !state.results_empty {
                update_action_rates(spot, game, &results, player_index);
            }
        }
    }

    // Update indices after all processing
    state.selected_spot_index = selected_spot_index_tmp;
    state.selected_chance_index = selected_chance_index_tmp;
    state.is_dealing = false;

    // println!("selected_spot_index: {}", state.selected_spot_index,);
    // println!("selected_chance_index: {}", state.selected_chance_index,);

    Ok(results)
}

/// Helper function to update action rates for a player spot
fn update_action_rates(
    spot: &mut Spot,
    game: &PostFlopGame,
    results: &SpecificResultData,
    player_index: usize,
) {
    // println!("update_action_rates() - spot_type: {:?}", spot.spot_type);

    // Vérifier si les résultats sont vides pour ce joueur
    if results.is_empty {
        // println!("Résultats vides, pas de mise à jour des taux");
        // Mettre tous les taux à -1 pour indiquer qu'ils sont indisponibles
        for action in spot.actions.iter_mut() {
            action.rate = -1.0;
        }
        return;
    }

    // Obtenir le nombre de mains pour ce joueur
    let n = game.private_cards(player_index).len();

    // Calculer les taux pour chaque action
    for (i, action) in spot.actions.iter_mut().enumerate() {
        // Extraire la tranche de stratégie pour cette action
        let start = i * n;
        let end = (i + 1) * n;

        if end <= results.strategy.len() {
            // Convertir les données pour weighted_average
            let strategy_slice: Vec<f32> = results.strategy[start..end]
                .iter()
                .map(|&v| v as f32)
                .collect();

            let normalizer_slice: Vec<f32> = results.normalizer[player_index]
                .iter()
                .map(|&v| v as f32)
                .collect();

            // Calculer la moyenne pondérée
            action.rate = weighted_average(&strategy_slice, &normalizer_slice) as f64;
        } else {
            // Stratégie incomplète, mettre le taux à -1
            action.rate = -1.0;
        }

        // println!("Action {}: {}, Taux: {:.4}", i, action.name, action.rate);
    }
}

fn process_from_deal(game: &mut PostFlopGame, state: &mut GameState) -> i32 {
    // println!(
    //     "process_from_deal() - selected_chance_index: {}",
    //     state.selected_chance_index
    // );

    let selected_chance_idx = state.selected_chance_index as usize;
    let find_river_index = state
        .spots
        .iter()
        .skip(selected_chance_idx + 3)
        .position(|spot| spot.spot_type == SpotType::Chance);

    let river_index: i32 = if let Some(idx) = find_river_index {
        (idx + selected_chance_idx + 3) as i32
    } else {
        -1
    };

    let river_spot = river_index != -1;
    let new_selected_chance_index = -1;

    // Si un nœud river existe, mettre à jour les cartes mortes
    if river_spot {
        let mut history = Vec::new();
        for i in 1..river_index as usize {
            if state.spots[i].selected_index != -1 {
                history.push(state.spots[i].selected_index as usize);
            }
        }
        game.apply_history(&history);

        let possible_cards = game.possible_cards();

        let river_spot = &mut state.spots[river_index as usize];
        for (i, card) in river_spot.cards.iter_mut().enumerate() {
            let is_dead = (possible_cards & (1u64 << i)) == 0;
            card.is_dead = is_dead;

            if river_spot.selected_index == i as i32 && is_dead {
                card.is_selected = false;
                river_spot.selected_index = -1;
            }
        }
    }

    let river_skipped = river_spot && state.spots[river_index as usize].selected_index == -1;
    let last_spot_idx = state.spots.len() - 1;

    if !river_skipped
        && state.spots[last_spot_idx].spot_type == SpotType::Terminal
        && state.spots[last_spot_idx].equity_oop != 0.0
        && state.spots[last_spot_idx].equity_oop != 1.0
    {
        let mut history = Vec::new();
        for i in 1..last_spot_idx {
            if state.spots[i].selected_index != -1 {
                history.push(state.spots[i].selected_index as usize);
            }
        }
        game.apply_history(&history);

        if let Ok(results) = get_specific_result(game, "terminal", 0) {
            if !results.is_empty {
                let equity_oop = weighted_average(
                    &results.equity[0]
                        .iter()
                        .map(|&x| x as f32)
                        .collect::<Vec<_>>(),
                    &results.normalizer[0]
                        .iter()
                        .map(|&x| x as f32)
                        .collect::<Vec<_>>(),
                ) as f64;

                state.spots[last_spot_idx].equity_oop = equity_oop;
            } else {
                state.spots[last_spot_idx].equity_oop = -1.0;
            }
        }
    }

    new_selected_chance_index
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
    // println!("splice_spots_terminal() - spot_index: {}", spot_index);
    let prev_spot = &state.spots[spot_index - 1];
    let prev_action = &prev_spot.actions[prev_spot.selected_index as usize];

    let chance_index = state.selected_chance_index;
    let chance_skipped =
        chance_index != -1 && state.spots[chance_index as usize].selected_index == -1;

    let equity_oop = if prev_action.name == "Fold" {
        if prev_spot.player == "oop" {
            0.0
        } else {
            1.0
        }
    } else if chance_skipped || state.results_empty {
        -1.0
    } else {
        let equity_vec: Vec<f32> = state.results.equity[0].iter().map(|&x| x as f32).collect();
        let normalizer_vec: Vec<f32> = state.results.normalizer[0]
            .iter()
            .map(|&x| x as f32)
            .collect();

        weighted_average(&equity_vec, &normalizer_vec) as f64
    };

    state.equity_oop = equity_oop;
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
) -> Result<(i32, i32), String> {
    // println!("splice_spots_chance() - spot_index: {}", spot_index);
    let prev_spot = &state.spots[spot_index - 1];
    let turn_spot = state
        .spots
        .iter()
        .take(spot_index)
        .find(|spot| spot.player == "turn");

    // println!("splice spots chance test 1");
    let mut append_array = Vec::new();
    if state.selected_chance_index != -1 {
        for i in state.selected_chance_index as usize..spot_index {
            append_array.push(state.spots[i].selected_index);
        }
    }

    // println!("splice spots chance test 2");

    let mut possible_cards = 0u64;
    if !(turn_spot.is_some()
        && turn_spot.unwrap().spot_type == SpotType::Chance
        && turn_spot.unwrap().selected_index == -1)
    {
        possible_cards = game.possible_cards();
    }

    // println!("splice spots chance test 3");
    append_array.push(-1);
    let append_array_usize: Vec<usize> = append_array
        .iter()
        .map(|&x| if x < 0 { 0 } else { x as usize })
        .collect();
    let next_actions_str = actions_after(game, &append_array_usize);
    let next_actions: Vec<&str> = next_actions_str.split('/').collect();

    // println!("splice spots chance test 4");
    let mut num_bet_actions = next_actions.len();
    while num_bet_actions > 0
        && next_actions[next_actions.len() - num_bet_actions]
            .split(':')
            .nth(1)
            .unwrap_or("1")
            == "0"
    {
        num_bet_actions -= 1;
    }

    let can_chance_reports = state.selected_chance_index == -1;
    state.can_chance_reports = can_chance_reports;

    // println!("splice spots chance test 5");
    if can_chance_reports {
        let num_actions = next_actions.len();
        match get_specific_chance_reports(game, &append_array_usize, "oop", num_actions) {
            Ok(reports) => {
                // println!("Rapports de chance obtenus avec succès");
                state.chance_reports = Some(reports);
            }
            Err(e) => {
                println!("Erreur lors de l'obtention des rapports de chance: {}", e);
            }
        }
    }

    // println!("splice spots chance test 6");
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

    // Créer le spot de joueur avec les actions (toujours OOP après un nœud de chance)
    let mut player_actions = Vec::new();
    for (i, action) in next_actions.iter().enumerate() {
        let parts: Vec<&str> = action.split(':').collect();
        let name = parts[0].to_string();
        let amount = if parts.len() > 1 {
            parts[1].to_string()
        } else {
            "0".to_string()
        };

        // Note: Dans le frontend, une couleur est calculée ici avec actionColor
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
        player: "oop".to_string(), // OOP joue toujours après un nœud de chance
        selected_index: -1,
        cards: Vec::new(),
        actions: player_actions,
        pot: new_chance_spot.pot,
        stack: new_chance_spot.stack,
        equity_oop: 0.0,
        prev_player: Some(new_chance_spot.player.clone()),
    };

    // Faire le splice comme dans le frontend
    state.spots.truncate(spot_index);
    state.spots.push(new_chance_spot);
    state.spots.push(new_player_spot);

    let new_selected_spot_index = spot_index as i32 + 1;
    let new_selected_chance_index = if state.selected_chance_index == -1 {
        spot_index as i32
    } else {
        state.selected_chance_index
    };

    // println!(
    //     "splice_spots_chance - new_selected_chance_index: {}, new_selected_spot_index: {}",
    //     new_selected_chance_index, new_selected_spot_index
    // );

    Ok((new_selected_chance_index, new_selected_spot_index))
}

/// Fonction pour mettre à jour les spots avec un nœud de joueur
/// Reproduction fidèle de spliceSpotsPlayer dans ResultNav.vue
fn splice_spots_player(
    state: &mut GameState,
    spot_index: usize,
    actions_str: String,
) -> Result<(), String> {
    // println!("splice_spots_player() - spot_index: {}", spot_index);
    let prev_spot = &state.spots[spot_index - 1];
    let player = if prev_spot.player == "oop" {
        "ip"
    } else {
        "oop"
    };

    let actions: Vec<&str> = actions_str.split('/').collect();
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
    action_index: usize,
) -> Result<SpecificResultData, String> {
    let spot_index = state.selected_spot_index as usize;

    // state.log_state("play()");
    // println!("play() - action_index: {}", action_index);
    // Récupérer le spot actuel

    let spot: &mut Spot = match state.spots.get_mut(spot_index) {
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

    spot.selected_index = action_index as i32;

    select_spot(game, state, spot_index + 1, true, false)
}

/// Function to deal a card at the currently selected chance node
pub fn deal(
    game: &mut PostFlopGame,
    state: &mut GameState,
    card_index: usize,
) -> Result<SpecificResultData, String> {
    // Check if there's a selected chance node
    if state.selected_chance_index == -1 {
        return Err("Aucun nœud de chance sélectionné".to_string());
    }

    // Get the chance spot
    let chance_index = state.selected_chance_index as usize;
    let chance_spot = &mut state.spots[chance_index];

    // Ensure it's a chance node
    if chance_spot.spot_type != SpotType::Chance {
        return Err("Le spot sélectionné n'est pas un nœud de chance".to_string());
    }

    // Check if the card is dead/unavailable
    if card_index >= chance_spot.cards.len() || chance_spot.cards[card_index].is_dead {
        return Err(format!(
            "La carte à l'index {} est morte ou invalide",
            card_index
        ));
    }

    // Mark that we're in a dealing state
    state.is_dealing = true;

    // Deselect the previously selected card if any
    if chance_spot.selected_index != -1 {
        chance_spot.cards[chance_spot.selected_index as usize].is_selected = false;
    }

    // Select the new card
    chance_spot.cards[card_index].is_selected = true;
    chance_spot.selected_index = card_index as i32;

    // Call select_spot to update the game state with from_deal=true
    select_spot(game, state, state.selected_spot_index as usize, false, true)
}

pub fn get_chance_reports(
    game: &mut PostFlopGame,
    append: &[usize],
    num_actions: usize,
) -> Box<[f64]> {
    let history = game.history().to_vec();

    let mut status = vec![0.0; 52]; // 0: not possible, 1: empty, 2: not empty
    let mut combos = [vec![0.0; 52], vec![0.0; 52]];
    let mut equity = [vec![0.0; 52], vec![0.0; 52]];
    let mut ev = [vec![0.0; 52], vec![0.0; 52]];
    let mut eqr = [vec![0.0; 52], vec![0.0; 52]];
    let mut strategy = vec![0.0; num_actions * 52];

    let possible_cards = game.possible_cards();
    for chance in 0..52 {
        if possible_cards & (1 << chance) == 0 {
            continue;
        }

        game.play(chance);
        for &action in &append[1..] {
            game.play(action);
        }

        let trunc = |&w: &f32| if w < 0.0005 { 0.0 } else { w };
        let weights = [
            game.weights(0).iter().map(trunc).collect::<Vec<_>>(),
            game.weights(1).iter().map(trunc).collect::<Vec<_>>(),
        ];

        combos[0][chance] = round(weights[0].iter().fold(0.0, |acc, &w| acc + w as f64));
        combos[1][chance] = round(weights[1].iter().fold(0.0, |acc, &w| acc + w as f64));

        let is_empty = |player: usize| weights[player].iter().all(|&w| w == 0.0);
        let is_empty_flag = [is_empty(0), is_empty(1)];

        game.cache_normalized_weights();
        let normalizer = [game.normalized_weights(0), game.normalized_weights(1)];

        if !game.is_terminal_node() {
            let current_player = game.current_player();
            if !is_empty_flag[current_player] {
                let strategy_tmp = game.strategy();
                let num_hands = game.private_cards(current_player).len();
                let ws = if is_empty_flag[current_player ^ 1] {
                    &weights[current_player]
                } else {
                    normalizer[current_player]
                };
                for action in 0..num_actions {
                    let slice = &strategy_tmp[action * num_hands..(action + 1) * num_hands];
                    let strategy_summary = weighted_average(slice, ws);
                    strategy[action * 52 + chance] = round(strategy_summary);
                }
            }
        }

        if is_empty_flag[0] || is_empty_flag[1] {
            status[chance] = 1.0;
            game.apply_history(&history);
            continue;
        }

        status[chance] = 2.0;

        let total_bet_amount = game.total_bet_amount();
        let pot_base = game.tree_config().starting_pot as f64
            + total_bet_amount
                .iter()
                .fold(0.0f64, |a, &b| a.min(b as f64));

        for player in 0..2 {
            let pot = pot_base + total_bet_amount[player] as f64;
            let equity_tmp = weighted_average(&game.equity(player), normalizer[player]);
            let ev_tmp = weighted_average(&game.expected_values(player), normalizer[player]);
            equity[player][chance] = round(equity_tmp);
            ev[player][chance] = round(ev_tmp);
            eqr[player][chance] = round(ev_tmp / (pot as f64 * equity_tmp));
        }

        game.apply_history(&history);
    }

    let mut buf = Vec::new();

    buf.extend_from_slice(&status);
    buf.extend_from_slice(&combos[0]);
    buf.extend_from_slice(&combos[1]);
    buf.extend_from_slice(&equity[0]);
    buf.extend_from_slice(&equity[1]);
    buf.extend_from_slice(&ev[0]);
    buf.extend_from_slice(&ev[1]);
    buf.extend_from_slice(&eqr[0]);
    buf.extend_from_slice(&eqr[1]);
    buf.extend_from_slice(&strategy);

    buf.into_boxed_slice()
}

pub fn get_specific_chance_reports(
    game: &mut PostFlopGame,
    append: &[usize],
    player: &str,
    num_actions: usize,
) -> Result<SpecificChanceReportData, String> {
    let buffer = get_chance_reports(game, append, num_actions);
    let mut offset = 0;

    let status = buffer[offset..offset + 52].to_vec();
    offset += 52;

    let combos_oop = buffer[offset..offset + 52].to_vec();
    offset += 52;
    let combos_ip = buffer[offset..offset + 52].to_vec();
    offset += 52;

    let equity_oop: Vec<f64> = buffer[offset..offset + 52].to_vec();
    offset += 52;
    let equity_ip = buffer[offset..offset + 52].to_vec();
    offset += 52;

    let ev_oop = buffer[offset..offset + 52].to_vec();
    offset += 52;
    let ev_ip = buffer[offset..offset + 52].to_vec();
    offset += 52;

    let eqr_oop = buffer[offset..offset + 52].to_vec();
    offset += 52;
    let eqr_ip = buffer[offset..offset + 52].to_vec();
    offset += 52;

    let strategy = if player != "terminal" {
        buffer[offset..offset + 52 * num_actions].to_vec()
    } else {
        Vec::new()
    };

    Ok(SpecificChanceReportData {
        current_player: player.to_string(),
        num_actions,
        status,
        combos: vec![combos_oop, combos_ip],
        equity: vec![equity_oop, equity_ip],
        ev: vec![ev_oop, ev_ip],
        eqr: vec![eqr_oop, eqr_ip],
        strategy,
    })
}
