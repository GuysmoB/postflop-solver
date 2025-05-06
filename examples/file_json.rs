use postflop_solver::{ActionData, TreeNode};
use std::{collections::HashMap, fs::File};

fn main() {
    print_hand_strategy("results.json", "F:Check-Check, T:4d", "TdJd").unwrap();
}

/// Fonction permettant d'extraire la stratégie pour une main spécifique à un chemin d'actions donné
pub fn get_hand_strategy(
    json_file: &str,
    path: &str,
    hand: &str,
) -> Result<Option<HashMap<String, ActionData>>, String> {
    // Charger le fichier JSON
    let file = File::open(json_file).map_err(|e| format!("Erreur ouverture fichier: {}", e))?;
    let tree: TreeNode =
        serde_json::from_reader(file).map_err(|e| format!("Erreur parsing JSON: {}", e))?;

    // Parcourir l'arbre pour trouver le nœud spécifié par le chemin
    let node = find_node_by_path(&tree, path)?;

    // Si le nœud est trouvé, chercher la stratégie pour la main spécifiée
    if let Some(strategy) = &node.strategy {
        if let Some(hand_strategy) = strategy.strategy.get(hand) {
            return Ok(Some(hand_strategy.actions.clone()));
        }
    }

    // Si la main n'est pas trouvée dans ce nœud
    Ok(None)
}

/// Fonction auxiliaire pour trouver un nœud par son chemin d'actions
fn find_node_by_path<'a>(root: &'a TreeNode, path: &str) -> Result<&'a TreeNode, String> {
    // Si le chemin est vide ou égal à "F:", nous sommes à la racine
    if path.is_empty() || path == "F:" {
        return Ok(root);
    }

    // Cas spécial pour la racine
    if root.path == path {
        return Ok(root);
    }

    // Recherche en profondeur pour trouver le nœud
    find_node_by_path_recursive(root, path)
}

/// Recherche récursive d'un nœud par son chemin
fn find_node_by_path_recursive<'a>(
    node: &'a TreeNode,
    target_path: &str,
) -> Result<&'a TreeNode, String> {
    // Si ce nœud correspond au chemin recherché
    if node.path == target_path {
        return Ok(node);
    }

    // Explorer les enfants
    for child in node.childrens.values() {
        // Vérifier si le chemin cible commence par le chemin de ce nœud
        if target_path.starts_with(&node.path) || node.path == "F:" {
            // Recherche récursive dans ce sous-arbre
            match find_node_by_path_recursive(child, target_path) {
                Ok(found) => return Ok(found),
                Err(_) => continue, // Continuer avec le prochain enfant
            }
        }
    }

    // Si on arrive ici, le nœud n'a pas été trouvé
    Err(format!("Chemin non trouvé: {}", target_path))
}

/// Fonction d'utilisation avec impression formatée
pub fn print_hand_strategy(json_file: &str, path: &str, hand: &str) -> Result<(), String> {
    match get_hand_strategy(json_file, path, hand)? {
        Some(actions) => {
            println!("=== Stratégie pour la main {} au chemin {} ===", hand, path);

            // Trier les actions par fréquence décroissante
            let mut sorted_actions: Vec<(&String, &ActionData)> = actions.iter().collect();
            sorted_actions.sort_by(|a, b| b.1.frequency.partial_cmp(&a.1.frequency).unwrap());

            // Afficher les actions dans l'ordre
            for (action, data) in sorted_actions {
                println!(
                    "  {} : {:.1}% (EV: {:.1} bb)",
                    action,
                    data.frequency * 100.0,
                    data.ev
                );
            }

            // Trouver l'action avec le meilleur EV
            if let Some((best_action, best_data)) = actions
                .iter()
                .max_by(|a, b| a.1.ev.partial_cmp(&b.1.ev).unwrap())
            {
                println!(
                    "\nMeilleure action par EV: {} (EV: {:.1} bb)",
                    best_action, best_data.ev
                );
            }

            Ok(())
        }
        None => {
            println!(
                "Aucune stratégie trouvée pour la main {} au chemin {}",
                hand, path
            );
            Ok(())
        }
    }
}

/// Exemple d'utilisation
pub fn example_usage() {
    match print_hand_strategy("results.json", "F:Check-Bet10", "AhAd") {
        Ok(_) => println!("Recherche effectuée avec succès."),
        Err(e) => eprintln!("Erreur: {}", e),
    }
}
