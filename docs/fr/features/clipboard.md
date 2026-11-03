[Documentation](../README.md) > Fonctionnalités > Presse-papiers | [English](../../en/features/clipboard.md)

# Presse-papiers

Couper, copier et coller des objets de la map, coller de nombreuses copies d'un coup avec le collage spécial, dupliquer la sélection, et copier les keyvalues d'une entité vers d'autres.

## Les commandes

Elles sont toutes dans le menu **Édition** et dans le menu du clic droit des vues :

| Commande | Raccourci | Effet |
|---|---|---|
| **Couper** | `Ctrl+X` | Copie les objets sélectionnés dans le presse-papiers et les supprime |
| **Copier** | `Ctrl+C` | Copie les objets sélectionnés dans le presse-papiers |
| **Coller** | `Ctrl+V` | Colle le contenu du presse-papiers ; les objets collés deviennent la sélection |
| **Collage spécial...** | `Ctrl+Maj+V` | Ouvre la boîte Collage spécial (plusieurs copies, décalage, rotation) |
| **Supprimer** | `Suppr` | Supprime les objets sélectionnés |
| **Dupliquer** | `Ctrl+D` | Copie la sélection à un pas de grille sur X et Y, sans passer par le presse-papiers ; les copies sont sélectionnées |
| **Copier les keyvalues** | aucun | Copie les keyvalues de l'entité sélectionnée |
| **Coller les keyvalues** | aucun | Les colle sur les entités sélectionnées |

## Où Coller place les objets

- `Ctrl+V` avec le pointeur sur une vue 2D : le contenu est centré sur le curseur et garde sa profondeur.
- `Ctrl+V` avec le pointeur sur la vue 3D : le contenu se pose sur la surface sous le curseur.
- Le décalage suit la grille : des objets calés sur la grille le restent.
- Ailleurs (menu, pointeur hors des vues) : les objets sont décalés d'un pas de grille sur X et Y si **Décaler les objets collés** est activé (par défaut), sinon collés à leur place d'origine.

## Ce que contient le presse-papiers

Hammer Pro utilise le presse-papiers du système, sous forme de texte VMF : des blocs `solid`, `entity` et `hidden` de premier niveau. Du coup :

- Vous pouvez copier entre deux fenêtres de Hammer Pro, et entre Hammer Pro et Hammer ou Hammer++, dans les deux sens.
- N'importe quel morceau de VMF, même un VMF complet, peut être collé. Les visgroups d'un VMF complet sont fusionnés par nom.
- Un bloc `hammerpro_clipboard` placé en tête (ignoré par les autres éditeurs) transporte les groupes : le collage garde les regroupements, même dans une autre instance de Hammer Pro.
- Les objets collés reçoivent de nouveaux id.

## Collage spécial

**Édition > Collage spécial...** (`Ctrl+Maj+V`) colle le presse-papiers plusieurs fois, chaque copie décalée et tournée par rapport à la précédente (comme le Paste Special de Hammer) :

| Option | Effet | Par défaut |
|---|---|---|
| **Copies** | Nombre de copies (1 à 10000 ; au-delà de 1000 la boîte prévient que la map s'alourdit beaucoup) | 1 |
| **Décalage par copie** | Décalage X, Y, Z ajouté à chaque copie. Le bouton `<` à côté d'un axe prend la taille du contenu du presse-papiers sur cet axe (copies côte à côte) | 64, 0, 0 |
| **Rotation par copie** | Rotation en degrés autour de X, Y et Z ajoutée à chaque copie, autour du centre des objets copiés | 0, 0, 0 |
| **Préfixe de nom** | Préfixe facultatif ajouté au nom de chaque entité nommée collée | vide |
| **Commencer à la position d'origine** | La première copie est placée exactement sur les objets copiés | Désactivé |
| **Grouper chaque copie** | Chaque copie devient un groupe | Désactivé |
| **Rendre les noms d'entités uniques** | Les entités collées dont le nom existe déjà sont renommées (`nom_2`, `nom_3`...) | Désactivé |
| **Pivoter autour du pivot** | La rotation tourne autour du pivot déplaçable au lieu du centre des objets copiés. Disponible seulement quand le pivot est affiché (**Pivot déplaçable**) | Activé |

- La boîte affiche **Décalage de la dernière copie** pour vérifier le total.
- Les entités renommées restent liées entre elles : les clés des entités collées qui nomment une autre entité collée (cibles, filtres) suivent les nouveaux noms.
- `Entrée` colle, `Échap` ferme. Les valeurs sont gardées pour la session.
- Tout le collage tient en une seule étape d'annulation.

## Dupliquer et cloner

**Dupliquer** (`Ctrl+D`) copie la sélection à un pas de grille sur X et Y. Avec l'[outil de sélection](selection-tool.md), vous pouvez aussi cloner en maintenant `Maj` pendant un glisser (déplacement, échelle, rotation, inclinaison, gizmo, étirement) ou avec `Maj+flèches`. Toutes ces copies restent dans les visgroups de leurs originaux si **Garder les groupes de visibilité au clonage** est activé (page **Sélection**, activé par défaut). Elles ne touchent pas au presse-papiers.

## Copier et coller les keyvalues

Sélectionnez une entité et faites **Édition > Copier les keyvalues**, puis sélectionnez d'autres entités et faites **Édition > Coller les keyvalues** : toutes les clés sont collées sauf `id`, `classname`, `origin` et `targetname`, en une seule étape d'annulation. Copier les keyvalues met aussi les clés dans le presse-papiers du système sous forme de lignes `"clé" "valeur"`, lisibles dans un éditeur de texte. Les deux commandes se trouvent aussi dans le menu du panneau d'entité. Activez **Coller les keyvalues colle aussi le nom (targetname)** (page **Entités et navigateurs**, désactivé par défaut) pour coller le nom avec. Voir [Propriétés des entités](entity-properties.md).

## Paramètres

| Option | Page | Effet | Par défaut |
|---|---|---|---|
| **Décaler les objets collés** | **Édition** | Le collage décale les objets d'un pas de grille pour qu'ils ne se superposent pas (quand le collage ne suit pas le curseur) | Activé |
| **Garder les groupes de visibilité au clonage** | **Sélection** | Les clones restent dans les visgroups de leurs originaux | Activé |
| **Coller les keyvalues colle aussi le nom (targetname)** | **Entités et navigateurs** | Coller les keyvalues copie aussi `targetname` | Désactivé |

## Astuces

- Pour aligner une rangée de pièces identiques, copiez-en une, ouvrez le collage spécial, cliquez sur `<` pour l'axe de la rangée et réglez le nombre de copies.
- Un escalier en colimaçon : une marche, une **Rotation par copie** de 15 sur Z et un **Décalage par copie** de 8 sur Z, avec le pivot sur l'axe de l'escalier.
- **Fichier > Exporter la sélection en VMF...** enregistre la sélection dans un fichier plutôt que dans le presse-papiers (voir [Compatibilité VMF](../getting-started/vmf-compatibility.md)).

## FAQ

**Je peux coller dans Hammer ?** Oui. Le presse-papiers contient des blocs VMF classiques que Hammer et Hammer++ savent lire. Le bloc des groupes y est simplement ignoré.

**Le collage dit que le presse-papiers ne contient pas d'objets de map.** Le presse-papiers contient autre chose (du texte, une image). Copiez d'abord des objets.

**Mes entités collées ont perdu leurs liens.** Utilisez **Rendre les noms d'entités uniques** ou un **Préfixe de nom** : les clés qui nomment d'autres entités collées suivent les nouveaux noms.

## Pages liées

- [Outil de sélection](selection-tool.md)
- [Transformer et aligner](transform-align.md)
- [Propriétés des entités](entity-properties.md)
- [Groupes](groups.md)
- [Visgroups](visgroups.md)
- [Annulation et historique](undo-history.md)
- [Référence des outils MCP (anglais)](../../MCP.md)

---
[Retour à l'index de la documentation](../README.md)
