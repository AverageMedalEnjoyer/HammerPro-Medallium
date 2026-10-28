[Documentation](../README.md) > Dépannage > Le statut Discord ne s'affiche pas | [English](../../en/troubleshooting/discord-presence.md)

# Le statut Discord ne s'affiche pas

Hammer Pro peut afficher ce que vous mappez dans votre profil Discord (la map, l'outil actif, le jeu et le temps passé sur la map), comme Hammer++. Si rien n'apparaît, suivez cette liste. La fonction elle-même est décrite dans [Statut Discord (Rich Presence)](../features/discord.md).

## Liste de vérification

1. **Un ID d'application est renseigné.** Hammer Pro n'a pas d'application Discord intégrée : tant que vous n'avez pas collé l'ID de la vôtre, rien n'est affiché, et la page **Discord** des paramètres indique « Aucun ID d'application : rien n'est encore affiché sur Discord. ». Voir plus bas pour en créer une.
2. **L'ID est valide.** C'est un nombre de 17 à 20 chiffres, pris sur la page **General Information** de l'application. Toute autre valeur est refusée (« Ce n'est pas un ID d'application Discord »).
3. **Afficher l'activité sur Discord est activé** (**Paramètres > Discord**, activé par défaut). Le désactiver efface l'activité aussitôt.
4. **L'application Discord pour ordinateur tourne sur ce PC.** Discord dans un navigateur ne peut pas recevoir le statut : Hammer Pro dialogue avec le client Discord local par ses canaux IPC (`\\.\pipe\discord-ipc-0` à `discord-ipc-9` sous Windows).
5. **Le partage d'activité est autorisé dans Discord.** Dans les paramètres de Discord, **Confidentialité des activités**, activez **Partager vos activités détectées**.
6. **Patientez un peu.** Les mises à jour partent au plus toutes les 15 secondes. Quand Discord n'est pas lancé, Hammer Pro réessaie discrètement, d'abord après 5 secondes, puis moins souvent (jusqu'à toutes les 60 secondes) : lancer Discord après Hammer Pro fonctionne, cela prend juste jusqu'à une minute.

Les lancements automatisés (`--script`, tests) ne contactent jamais Discord.

## Créer l'application Discord

1. Ouvrez le Discord Developer Portal et créez une application nommée « Hammer Pro » (c'est ce nom qu'affiche votre profil).
2. Dans **Rich Presence > Art Assets**, envoyez le logo de Hammer Pro avec la clé `hammerpro` (sans lui, l'activité n'a pas d'image).
3. Sur la page **General Information**, copiez l'**Application ID**.
4. Dans Hammer Pro, **Outils > Options...** (`Ctrl+,`), page **Discord**, collez-le dans **ID de l'application Discord**.

## La page Discord des paramètres

| Option | Effet | Défaut |
|---|---|---|
| **Afficher l'activité sur Discord** | Active ou coupe le statut. | Activé |
| **ID de l'application Discord** | L'ID de votre application Discord. | Vide |
| **Afficher le nom de la map** | Désactivé : « Édite une map » au lieu du nom de la map. | Activé |
| **Afficher l'outil actif** | Désactivé : le jeu cible est affiché à la place (et « Compilation... » pendant une compilation). | Activé |
| **Afficher le temps écoulé** | Le temps passé sur la map. | Activé |
| **Lien du bouton « Obtenir Hammer Pro »** | Page web facultative (`https://...`) affichée en bouton sous l'activité ; vide : pas de bouton. | Vide |

Ces réglages sont enregistrés dans la table `[discord]` de `settings.toml` (`enabled`, `application_id`, `show_map`, `show_tool`, `show_elapsed`, `project_url`).

## FAQ

**L'activité s'affiche mais sans le logo.** La clé de l'image doit être exactement `hammerpro` dans les Art Assets de Rich Presence de votre application. Discord peut mettre quelques minutes à rendre de nouvelles images disponibles.

**Les autres ne voient pas le nom de la map.** Vérifiez **Afficher le nom de la map**. Désactivé, Discord affiche « Édite une map ».

**Ça marchait, puis plus rien après un redémarrage de Discord.** Hammer Pro se reconnecte tout seul en moins d'une minute ; rien à faire.

## Pages liées

- [Statut Discord (Rich Presence)](../features/discord.md)
- [Fenêtre des paramètres](../features/settings-dialog.md)
- [Référence des paramètres](../reference/settings.md)
- [FAQ](faq.md)

---
[Retour à l'index de la documentation](../README.md)
