use hp_app::i18n::TrTable;

pub const TR: TrTable = &[
    ("goto.title", "Go to", "Aller à"),
    ("goto.prompt", "Object id or x y z coordinates", "Id d'objet ou coordonnées x y z"),
    ("goto.hint", "for example 1234 or 0 0 512", "par exemple 1234 ou 0 0 512"),
    ("goto.go", "Go", "Aller"),
    ("goto.invalid", "Type an object id or three coordinates", "Saisissez un id d'objet ou trois coordonnées"),
    ("goto.not_found", "No solid or entity with id {0}", "Aucun solide ni entité avec l'id {0}"),
    (
        "goto.hidden",
        "The object is hidden: the views are centered on it. Show it to select it.",
        "L'objet est masqué : les vues sont centrées dessus. Affichez-le pour le sélectionner.",
    ),
    ("goto.preview_solid", "Solid {0}", "Solide {0}"),
    ("goto.preview_entity", "Entity {0} ({1})", "Entité {0} ({1})"),
    ("goto.preview_position", "Center the views on {0}", "Centrer les vues sur {0}"),
];
