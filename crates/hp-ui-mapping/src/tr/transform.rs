use hp_app::i18n::TrTable;

pub const TR: TrTable = &[
    ("transform.title", "Transform", "Transformer"),
    ("transform.move", "Move", "Déplacer"),
    ("transform.rotate", "Rotate", "Pivoter"),
    ("transform.scale", "Scale", "Mettre à l'échelle"),
    ("transform.offset", "Offset", "Décalage"),
    ("transform.angles", "Angles (degrees)", "Angles (degrés)"),
    ("transform.factors", "Factors", "Facteurs"),
    ("transform.pivot", "Pivot", "Pivot"),
    ("transform.pivot_selection", "Selection center", "Centre de la sélection"),
    ("transform.pivot_origin", "World origin", "Origine du monde"),
    ("transform.pivot_custom", "Movable pivot", "Pivot déplaçable"),
    ("transform.pivot_custom_tip", "Around the movable pivot at {0}", "Autour du pivot déplaçable en {0}"),
    (
        "transform.pivot_custom_off",
        "Show the movable pivot (Map menu) to transform around it",
        "Affichez le pivot déplaçable (menu Map) pour transformer autour de lui",
    ),
    ("transform.texture_lock", "Texture lock", "Verrouillage des textures"),
    ("transform.toggle", "Toggle", "Basculer"),
    ("transform.selection_info_one", "{0} object selected", "{0} objet sélectionné"),
    ("transform.selection_info", "{0} objects selected", "{0} objets sélectionnés"),
    (
        "transform.no_selection",
        "Select the objects to transform first",
        "Sélectionnez d'abord les objets à transformer",
    ),
    ("transform.failed", "Transform failed: {0}", "La transformation a échoué : {0}"),
    ("transform.err_invalid", "Invalid values", "Valeurs invalides"),
    ("transform.err_zero_scale", "Scale factors cannot be zero", "Les facteurs d'échelle ne peuvent pas être nuls"),
];
