use hp_app::i18n::TrTable;

pub const TR: TrTable = &[
    (
        "outliner.search_hint",
        "Search a name, class, id or material",
        "Rechercher un nom, une classe, un id ou un matériau",
    ),
    ("outliner.expand_all", "Expand all", "Tout déplier"),
    ("outliner.collapse_all", "Collapse all", "Tout replier"),
    (
        "outliner.summary",
        "World solids: {0}, entities: {1}, selected: {2}",
        "Solides du monde : {0}, entités : {1}, sélection : {2}",
    ),
    (
        "outliner.keys_tip",
        "Click a row to select its visible objects. With the pointer over the list, the arrow keys move the selection.",
        "Cliquez sur une ligne pour sélectionner ses objets visibles. Avec le pointeur sur la liste, les flèches déplacent la sélection.",
    ),
    (
        "outliner.hidden_not_selectable",
        "Hidden objects cannot be selected: show them first (eye button)",
        "Les objets masqués ne peuvent pas être sélectionnés : affichez-les d'abord (bouton œil)",
    ),
    ("outliner.no_match", "Nothing matches the search", "Aucun résultat pour cette recherche"),
    ("outliner.hide", "Hide", "Masquer"),
    ("outliner.unhide", "Show", "Afficher"),
    ("outliner.world", "World", "Monde"),
    ("outliner.entities", "Entities", "Entités"),
    ("outliner.group", "Group #{0}", "Groupe #{0}"),
    ("outliner.solid", "Solid #{0}", "Solide #{0}"),
];
