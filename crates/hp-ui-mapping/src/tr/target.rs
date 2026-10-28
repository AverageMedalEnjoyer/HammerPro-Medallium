use hp_app::i18n::TrTable;

pub const TR: TrTable = &[
    ("target.label", "Target game", "Jeu cible"),
    (
        "target.tip",
        "The game this map is for, kept with the map: the compile writes its BSP format and checks its limits, and its content and entity definitions are loaded when it is installed. The default game is in Settings > Game.",
        "Le jeu pour lequel cette map est faite, gardé avec la map : la compilation écrit son format de BSP et vérifie ses limites, et son contenu et ses définitions d'entités sont chargés quand il est installé. Le jeu par défaut se règle dans Paramètres > Jeu.",
    ),
    ("target.searching", "Searching the Steam libraries...", "Recherche dans les bibliothèques Steam..."),
    ("target.format", "BSP version {0}, static props {1}.", "BSP version {0}, props statiques {1}."),
    ("target.valve_limits", "Valve's limits.", "Limites de Valve."),
    (
        "target.not_installed",
        "Not installed: compiled with the content mounted now, not copied to a game nor played.",
        "Non installé : compilé avec le contenu monté actuellement, ni copié dans un jeu ni lancé.",
    ),
];
