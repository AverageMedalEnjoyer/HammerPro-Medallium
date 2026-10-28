use hp_app::i18n::TrTable;

pub const TR: TrTable = &[
    ("pointfile.title_one", "Leak path: {0} point", "Chemin de fuite : {0} point"),
    ("pointfile.title", "Leak path: {0} points", "Chemin de fuite : {0} points"),
    ("pointfile.empty", "The pointfile has no points", "Le pointfile ne contient aucun point"),
    ("pointfile.position", "Point {0} of {1}", "Point {0} sur {1}"),
    ("pointfile.first", "Point 1: the leaking entity", "Point 1 : l'entité qui fuit"),
    ("pointfile.previous", "Previous", "Précédent"),
    ("pointfile.previous_tip", "Previous point, toward the leaking entity", "Point précédent, vers l'entité qui fuit"),
    ("pointfile.next", "Next", "Suivant"),
    ("pointfile.next_tip", "Next point, toward the void", "Point suivant, vers le vide"),
    ("pointfile.at_entity", "Leak start: entity", "Début de la fuite : entité"),
    (
        "pointfile.leak_entity_gone",
        "The leaking entity is gone: leak path unloaded. Compile again to check the map is sealed.",
        "L'entité qui fuyait n'est plus là : chemin de fuite déchargé. Recompile pour vérifier que la map est étanche.",
    ),
    ("pointfile.at_void", "Leak end: void", "Fin de la fuite : vide"),
    ("pointfile.view_current", "Look at the current point", "Regarder le point actuel"),
    ("pointfile.unload", "Unload", "Décharger"),
];
