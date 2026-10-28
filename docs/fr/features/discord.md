[Documentation](../README.md) > Fonctionnalités > Statut Discord | [English](../../en/features/discord.md)

# Statut Discord (Rich Presence)

Comme Hammer++, Hammer Pro peut afficher ce que vous mappez dans votre profil Discord : la map, l'outil actif, le jeu et le temps passé sur la map.

## Où le régler

**Paramètres > Discord** (**Outils > Options...**, `Ctrl+,`), section **Statut Discord (Rich Presence)**. Les réglages sont enregistrés dans `[discord]` de settings.toml.

## Options

| Option | Effet | Défaut |
| --- | --- | --- |
| **Afficher l'activité sur Discord** | Active le statut. Le désactiver efface l'activité aussitôt. Clé `enabled`. | activé |
| **Afficher le nom de la map** | Affiche *Édite gm_city.vmf* ; désactivé, *Édite une map*. Clé `show_map`. | activé |
| **Afficher le temps écoulé** | Affiche le temps passé sur la map. Clé `show_elapsed`. | activé |
| **Afficher l'outil actif** | Affiche l'outil utilisé. Désactivé : le jeu cible est affiché à la place (et *Compilation...* pendant une compilation). Clé `show_tool`. | activé |
| **ID de l'application Discord** | ID de l'application Discord qui affiche l'activité (section **Application Discord**). Clé `application_id`. | vide |
| **Lien du bouton « Obtenir Hammer Pro »** | Page web facultative (`https://...`) affichée en bouton **Obtenir Hammer Pro** sous l'activité ; vide : pas de bouton. Clé `project_url`. | vide |

## Ce que Discord affiche

| Ligne | Contenu |
| --- | --- |
| Détails | *Édite map.vmf*, *Édite une map* (nom masqué) ou *Aucune map ouverte*. |
| État | L'outil actif, ou *Compilation...* pendant une compilation, ou le nom du jeu. |
| Durée | Le temps depuis l'ouverture de la map (avec **Afficher le temps écoulé**). |
| Grande image | Le logo Hammer Pro (clé d'image `hammerpro`), avec « Hammer Pro » et la version. |
| Petite image | L'icône du jeu de la map, avec son nom. |

## Configurer l'ID de l'application

Discord a besoin d'une application pour afficher un statut. Hammer Pro 1.0.0 n'a pas d'ID d'application intégré : tant que vous n'en mettez pas, rien ne s'affiche (*Aucun ID d'application : rien n'est encore affiché sur Discord.*).

1. Sur le Discord Developer Portal, créez une application nommée « Hammer Pro ».
2. Dans **Rich Presence > Art Assets**, envoyez le logo Hammer Pro avec la clé `hammerpro`.
3. Facultatif : envoyez des icônes de jeux avec ces clés : `gmod`, `hl2` (Half-Life 2, épisodes, Lost Coast, SDK 2013 solo), `hls` (Half-Life: Source), `hl2dm`, `tf2`, `css`, `dods`, `portal`, `portal2`, `l4d`, `l4d2`, `csgo`. Les autres jeux utilisent leur propre id de jeu comme clé.
4. Copiez l'**Application ID** (page General Information) dans **ID de l'application Discord**.

L'ID est un nombre de 17 à 20 chiffres ; toute autre valeur est refusée avec un message.

## Prérequis

L'application Discord de bureau doit tourner sur le même ordinateur, avec **Partager vos activités détectées** activé dans les paramètres **Confidentialité des activités** de Discord. Hammer Pro lui parle directement par la connexion locale de Discord, sur un thread de fond : quand Discord n'est pas lancé, il réessaie discrètement et l'éditeur ne l'attend jamais. Les sessions automatisées (`--script`, tests) n'atteignent jamais votre Discord.

## Astuces

- Désactivez **Afficher le nom de la map** quand vous travaillez sur un projet secret ; le statut indique alors juste *Édite une map*.
- Partagez un lien vers la page Workshop de votre map ou votre communauté avec le lien du bouton.

## FAQ

**Rien ne s'affiche sur Discord.** Vérifiez qu'un ID d'application est saisi, que l'application Discord de bureau tourne et que le partage d'activité est activé. Voir [Le statut Discord ne s'affiche pas](../troubleshooting/discord-presence.md).

**Le statut ralentit-il l'éditeur ?** Non, il tourne sur son propre thread et les mises à jour sont espacées.

**Ça marche avec Discord dans le navigateur ?** Non, le statut nécessite l'application de bureau sur le même ordinateur.

## Pages liées

- [Le statut Discord ne s'affiche pas](../troubleshooting/discord-presence.md)
- [Fenêtre des paramètres](settings-dialog.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)
