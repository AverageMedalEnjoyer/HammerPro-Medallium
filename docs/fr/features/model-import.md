[Documentation](../README.md) > Fonctionnalités > Import de modèles | [English](../../en/features/model-import.md)

# Import de modèles

L'import de modèles transforme un fichier 3D (FBX, OBJ, glTF ou GLB) en prop Garry's Mod sans Crowbar ni les compilateurs de Valve : Hammer Pro convertit ses textures en VTF et VMT, compile le modèle avec son compilateur intégré, en fait le modèle des nouveaux props et peut en poser un devant la caméra.

## Ouvrir l'import

- **Fichier > Importer un modèle (FBX, OBJ, glTF)...** ouvre une boîte de dialogue de fichier (filtre « Modèles 3D » : `.fbx`, `.obj`, `.gltf`, `.glb`).
- Le bouton **+** du [navigateur de modèles](model-browser.md) fait la même chose.
- Glissez-déposez un fichier `.fbx`, `.obj`, `.gltf` ou `.glb` sur la fenêtre de Hammer Pro. Un modèle à la fois : un second dépôt pendant un import est refusé avec un message.
- Les fichiers `--script` ont `import_model CHEMIN`, `import_model_now CHEMIN` (importe avec les options par défaut) et `import_model_scaled ÉCHELLE CHEMIN` (voir [Commandes de script](../reference/script-commands.md)).

Le fichier est lu en arrière-plan (« Lecture de ... »), puis la fenêtre **Importer un modèle** affiche ses options.

## Fichiers acceptés

| Format | Ce qui est lu |
|---|---|
| Wavefront OBJ (`.obj` avec son `.mtl`) | Maillages et matériaux. Les faces sans normales reçoivent des normales lissées par groupe de lissage (`s off` donne du plat), ou à travers les arêtes pas trop vives quand le fichier n'a pas de groupe de lissage. |
| glTF 2.0 (`.gltf`, `.glb`) | Les maillages de la scène par défaut avec les transformations de leurs nœuds (maillages skinnés en pose de référence) ; la couleur de base des matériaux (texture avec son jeu de coordonnées et `KHR_texture_transform`, facteur de couleur, mode d'alpha, double face). Les maillages compressés Draco ou meshopt sont refusés avec un message. |
| FBX 7.x, binaire et ASCII | Les maillages placés par leurs modèles (translation, rotation et son ordre, pivots, décalages, échelle, transformation géométrique, parents), les matériaux (couleur, texture diffuse en fichier ou intégrée), le système d'axes et l'unité du fichier. |

Seule la géométrie statique est importée : les animations sont ignorées. Les textures peuvent être en PNG, JPG, TGA ou BMP.

## Options

La fenêtre affiche d'abord le chemin du fichier et un résumé : triangles, matériaux et taille en unités. Des avertissements apparaissent au-delà de 65000 triangles (un prop lourd), et pour un modèle de moins de 2 unités ou de plus de 4096 unités (vérifiez l'échelle).

| Option | Effet | Défaut |
|---|---|---|
| **Nom** | `models/hammerpro/<nom>.mdl`. Nettoyé en lettres minuscules, chiffres et `_` (48 caractères au plus). Un avertissement signale qu'un modèle de ce nom existe : il est remplacé. | Le nom du fichier |
| **Type** | **prop_static**, ou **Prop physique** (`prop_physics`). | prop_static |
| **Échelle** | Échelle supplémentaire après la conversion d'unités (0,001 à 1000). | 1 |
| **Convertir les unités** | Les mètres du fichier deviennent des unités Source (pouces) : 1 m = 39,37 unités. Désactivé : une unité du fichier vaut une unité Source. | Activé |
| **Axe vertical** | **Celui du fichier**, **Y** ou **Z**. Les modèles en Y vertical sont tournés pour que Z soit vertical, l'avant restant devant. | Celui du fichier |
| **Origine** - **En bas au centre** | Met l'origine du modèle au centre de sa base. | Activé |
| **Propriété de surface** | Matériau physique et sonore : `default`, `concrete`, `metal`, `solidmetal`, `metal_box`, `metalpanel`, `wood`, `wood_crate`, `wood_furniture`, `plastic`, `plastic_barrel`, `glass`, `rock`, `dirt`, `grass`, `gravel`, `brick`, `tile`, `carpet`, `cardboard`, `paper`, `rubber`, `flesh`, `ice`, `porcelain`, `computer`, `pottery`. | default |
| **Collision** | **Morceaux** : chaque morceau séparé du modèle reçoit une enveloppe convexe. **Boîte** : la boîte du modèle. **Aucune** : pas de modèle de collision. | Morceaux |
| **Masse** (prop physique seulement) | 0,1 à 50000 kg. | 20 kg |
| **Niveaux de détail (LOD)** | Versions simplifiées du modèle que le jeu affiche de loin. Partagé avec [Propper](propper.md) et enregistré (`[lab.model_lod]`). | Activé |
| **Niveaux** | Niveaux sous le modèle complet, de 3 à 6 (les gros modèles en ont un ou deux de plus). | 4 |
| **Chacun garde** | Part des triangles du niveau précédent que garde chaque niveau, de 20 à 90 % (les petits modèles sont réduits plus doucement). | 50 % |
| **Le placer devant la caméra** | Après l'import, pose un prop du modèle là où regarde la caméra 3D. | Activé |

Un prop physique sans collision affiche un avertissement : il ne peut pas bouger en jeu.

**Matériaux** liste les matériaux utilisés par le modèle avec leur texture : le nom du fichier, « texture dans le fichier » (intégrée), ou « pas de texture (sa couleur) ».

Les options autres que le nom et les réglages de LOD restent comme vous les avez laissées jusqu'à la fermeture de Hammer Pro. **Importer** est grisé tant que le nom est vide.

## Ce qui est écrit

Le modèle va dans le dossier du jeu du contenu monté, ses textures à côté :

- `materials/models/hammerpro/<nom>/` : un VTF (avec mipmaps) et un VMT `VertexLitGeneric` par matériau utilisé. Un matériau sans texture (ou dont la texture est illisible) reçoit une petite texture de sa couleur, avec un avertissement dans la console. Les matériaux double face reçoivent `$nocull` ; l'alpha devient `$alphatest` ou `$translucent` (selon le mode d'alpha du matériau, ou selon l'alpha de la texture quand le fichier ne le précise pas) ; un matériau texturé teinté reçoit `$color2`, une couleur transparente `$alpha`.
- `models/hammerpro/<nom>.mdl` avec ses `.vvd`, `.dx90.vtx` et `.phy` (pas de `.phy` avec la collision **Aucune**). Les anciens fichiers `.dx80.vtx`, `.sw.vtx`, `.vtx` et `.ani` du même nom sont supprimés. Le MDL est écrit en dernier, pour que le jeu ne voie jamais un modèle à moitié écrit.

Le compilateur intégré produit ce que studiomdl fait d'un prop statique : un os, une séquence idle, un maillage par matériau, les niveaux de détail, et un LOD d'ombre pour les props physiques. Un modèle de plus de 65536 sommets est découpé en plusieurs parties (la console indique combien). Les enveloppes de collision sont réduites de la marge de collision comme celles de studiomdl ; les morceaux plats sont épaissis à une unité.

**Construire et enregistrer en contenu** embarque ces fichiers avec la map (voir [Construire et publier le contenu](build-content-release.md)).

## Après l'import

- Le contenu est remonté et une notification annonce « *modèle* est prêt (N textures, N morceaux de collision) : c'est le modèle des nouveaux props (outil Entité) ». La console liste les niveaux de détail avec leur nombre de triangles.
- Le modèle devient le modèle des nouveaux props, avec la classe `prop_static` ou `prop_physics`, pour l'[outil entité](entity-tool.md).
- Avec **Le placer devant la caméra**, un prop est créé là où regarde la caméra 3D : posé sur la surface touchée (son origine est au centre de sa base), sinon en l'air devant la caméra. Il devient la sélection (une seule étape d'annulation).

## Astuces

- Si la taille du résumé paraît 39 fois trop grande ou trop petite, basculez **Convertir les unités** ou changez l'**Échelle**.
- Restez sous 32 matériaux : un modèle du jeu ne peut pas en avoir plus et l'import s'arrête avec un message.
- Pour vérifier le résultat, sélectionnez le modèle dans le navigateur de modèles : l'onglet **Inspecteur** liste ses fichiers et matériaux.

## FAQ

**« Le contenu du jeu n'est pas chargé » : que faire ?** L'import écrit dans le dossier du jeu du contenu monté. Indiquez le dossier du jeu dans les paramètres et attendez la fin du chargement (voir [Configuration du jeu](../getting-started/game-setup.md)).

**« Le modèle est trop gros pour le jeu ».** Réduisez ses détails dans votre logiciel 3D et importez-le à nouveau.

**Mon glTF est refusé.** Les maillages compressés Draco et meshopt ne sont pas lus : exportez-le sans compression de maillage.

**Puis-je importer des animations ou des squelettes ?** Non : seule la géométrie statique est lue.

## Pages liées

- [Navigateur de modèles](model-browser.md)
- [Propper](propper.md)
- [Outil entité](entity-tool.md)
- [Construire et publier le contenu](build-content-release.md)
- [Dossiers de textures personnalisés](custom-texture-folders.md)
- [Commandes de script](../reference/script-commands.md)

---
[Retour à l'index de la documentation](../README.md)
