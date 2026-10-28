[Documentation](../README.md) > Fonctionnalités > Particules | [English](../../en/features/particles.md)

# Particules

Hammer Pro a un navigateur de particules qui liste tous les systèmes de particules des fichiers de particules montés (`.pcf`), et un aperçu des particules qui joue les effets des entités `info_particle_system` dans les vues 3D, comme Hammer++.

## Navigateur de particules

### Ouvrir le navigateur

- **Affichage > Panneaux > Navigateur de particules** ajoute l'onglet **Particules** au panneau de droite (ou le retire). Enregistré dans `show_particle_browser` de `[browser]` (désactivé par défaut).
- Le bouton de parcours d'une clé de particules (`effect_name` de `info_particle_system`) dans le panneau d'entité l'ouvre en mode choix.
- Pas de raccourci par défaut. Avec le préréglage de touches Hammer++ (**Paramètres > Clavier**), c'est `Maj+F11`.
- Le bouton `⊞` (**Ouvrir dans une fenêtre**) sort le navigateur dans une fenêtre flottante ; `⊟` (**Remettre dans le panneau**) le remet en place.

### Options

| Contrôle | Effet | Défaut |
|---|---|---|
| **Grille** / **Liste** | Grille de vignettes, ou une ligne par système avec une petite vignette, son fichier de particules et son badge de source. Gardé pendant la session. | Grille |
| Curseur de taille (grille seulement) | **Taille des vignettes** des grilles de particules et de modèles, de 48 à 256 points par pas de 8. `tile_size` dans `[browser]`. | 96 |
| **Rechercher des systèmes de particules (!mot exclut)** | Garde les systèmes dont le nom ou le fichier contient tous les mots ; `!mot` exclut. | |
| Liste des fichiers de particules | **Tous les fichiers de particules**, ou les systèmes d'un seul fichier. | Tous |
| Pastilles de source **Tout**, **Jeu**, **Addons**, **Workshop** | Ne gardent que les systèmes d'un type de source (une pastille n'apparaît que si le contenu l'a). | Tout |
| **Contenu de Garry's Mod uniquement** | Seulement ce que tous les joueurs de Garry's Mod possèdent. Partagé avec les navigateurs de modèles et de sons (`gmod_only` dans `[browser]`). | Désactivé |
| **Systèmes : N** | Nombre de systèmes listés (« Lecture des fichiers de particules... » pendant le chargement). | |

Les vignettes montrent le matériau du système (un `?` s'il n'en a pas ou s'il manque). Survolez un système pour voir son nom, son fichier, son matériau et sa source.

### Utiliser un système

- Un clic sélectionne un système.
- Double-clic (ou `Entrée` avec le pointeur sur la liste) : en mode choix, écrit le système dans la clé ; sinon l'applique aux entités `info_particle_system` sélectionnées.
- En bas : le nom sélectionné, son fichier et sa source, **Appliquer à la sélection (N)** (met le système dans `effect_name` des `info_particle_system` sélectionnées, une seule étape d'annulation) et **Copier le nom**.

### Choisir pour une clé

Une clé de particules du panneau d'entité (voir [Propriétés des entités](entity-properties.md)) a un bouton `▾` qui liste les systèmes de particules du contenu du jeu avec leur fichier (filtrable, et vous pouvez taper une valeur absente de la liste), et un bouton de parcours (« Choisir dans le navigateur de particules (double-cliquez sur un système) »).

En mode choix, le navigateur affiche « Choix pour *clé* : double-cliquez sur un élément, ou sélectionnez-le puis Utiliser. » avec la valeur actuelle sélectionnée. Double-clic, `Entrée` ou **Utiliser** écrit le système (une seule étape d'annulation) et revient sur l'onglet **Entité** ; **Annuler** quitte le mode choix.

## Aperçu des particules dans les vues

### L'activer

| Option | Où | Effet | Défaut |
|---|---|---|---|
| **Afficher les particules** | **Affichage > Options d'affichage**, et **Paramètres > Nav mesh et particules** | Joue les effets des `info_particle_system` proches de la caméra dans les vues 3D. `show_particles` dans `[lab]`. | Activé |
| **Particules de la sélection uniquement** | Aux mêmes endroits | Ne joue que les effets des systèmes de particules sélectionnés. `particles_selected_only` dans `[lab]`. | Désactivé |

### Ce qui est joué

- Les définitions viennent des fichiers de particules du contenu monté : d'abord les fichiers du manifeste de particules de la map (`maps/<map>_particles.txt`), puis le catalogue de particules du contenu. Ils sont lus en arrière-plan, avec l'aspect de leurs matériaux (`$additive`, `$overbrightfactor`, `$addself`, tailles de fondu, la feuille de sprites de la texture de base). Les systèmes enfants sont chargés aussi.
- Chaque effet est joué quel que soit son état de départ (`start_active`), à ses points de contrôle : l'entité elle-même avec ses angles, et les entités nommées par ses clés `cpoint1` à `cpoint63`. Les effets continus démarrent déjà lancés (1,5 seconde d'avance).
- Pour que le coût reste faible quelle que soit la taille de la map, seuls les effets à moins de 4096 unités de la caméra sont joués, 24 au plus en même temps (les plus proches d'abord), avec 3000 particules au plus en tout et 400 par effet.
- Les particules sont dessinées par le moteur de rendu avec l'image de leur feuille de sprites, face à l'écran, verticales, à plat ou étirées (traînées, cordes), en mélange ou en additif. L'aperçu reste une approximation : certains opérateurs des systèmes de particules ne sont pas simulés.
- Dans la [vue joueur](player-view.md) avec le monde compilé, ce sont les systèmes de particules de la map compilée qui sont joués.

Survolez ou sélectionnez un `info_particle_system` dans une vue 3D pour voir le nom de son effet au-dessus, avec son état : rien de plus s'il est joué, « (à l'arrêt : trop loin ou trop nombreux) », « (chargement...) » ou « (introuvable dans les fichiers de particules) ».

## Astuces

- Sur une map avec beaucoup d'effets, activez **Particules de la sélection uniquement** pour regarder un effet à la fois.
- Via MCP, `search_particles` cherche dans les systèmes du contenu et `list_particle_systems` donne l'état de chaque `info_particle_system` de la map (voir la [référence des outils MCP](../../MCP.md), en anglais).

## FAQ

**Un effet indique « introuvable dans les fichiers de particules ».** Aucun fichier de particules monté ne définit ce nom. Vérifiez l'orthographe, montez le jeu ou l'addon qui le contient, ou ajoutez son fichier au manifeste de particules de la map.

**Un effet n'est pas joué alors qu'il est dans le champ.** Il est à plus de 4096 unités, ou 24 effets plus proches sont déjà joués. Rapprochez la caméra.

**Pourquoi le navigateur ne liste rien ?** Le contenu du jeu n'est pas chargé, ou la recherche, le filtre de fichier, les pastilles de source ou **Contenu de Garry's Mod uniquement** masquent tout.

## Pages liées

- [Propriétés des entités](entity-properties.md)
- [Modes de rendu et affichage](render-modes-display.md)
- [Vue joueur](player-view.md)
- [Navigateur de modèles](model-browser.md)
- [Navigateur de sons](sound-browser.md)
- [Performances de l'éditeur](../troubleshooting/performance.md)

---
[Retour à l'index de la documentation](../README.md)
