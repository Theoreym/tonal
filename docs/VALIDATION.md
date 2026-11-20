# Validation Windows — 11 novembre 2026

## Environnement observé

- Windows x64, toolchain `stable-x86_64-pc-windows-msvc`.
- `rustc 1.99.1`, `cargo 1.99.1`.
- Visual Studio Build Tools 2022, MSVC 14.44 et SDK Windows 10.0.26100.
- yt-dlp 2026.10.26, FFmpeg 8.0.1 ; runtime Edge WebView2 143.0.3650.96 présent.
- Rust, les outils C++ / SDK et yt-dlp ont été installés pendant cette intervention. Les dépendances winget de yt-dlp incluent aussi Deno et son FFmpeg.

Le blocage initial de compilation était lié au SDK manquant, puis à Smart App Control (Code Integrity, erreur 4551). L'utilisateur a modifié ce dernier réglage dans Windows. Aucun script du projet ne modifie la sécurité Windows.

## Contrôles exécutés

- `cargo check` : réussi.
- `cargo fmt -- --check` : réussi après formatage.
- `cargo clippy --all-targets --locked -- -D warnings` : réussi, sans avertissement.
- `cargo build --release --locked` : réussi.
- `cargo test --locked -- --include-ignored --test-threads=1` : **12 tests réussis, 0 échec**.
- `powershell.exe -NoProfile -File .\scripts\run.ps1` : release démarrée avec Windows PowerShell 5.1. Processus répondant, fenêtre `Tonal — Music`.

`Cargo.lock` est inclus. Le binaire généré est `target\release\tonal.exe`.

## Essais réels inclus dans les 12 tests

### Audio local

FFmpeg a généré de courts signaux WAV, FLAC, MP3 et OGG. Chaque fichier a été chargé par le moteur de Tonal sur la sortie système. Les assertions vérifient la consommation effective des échantillons, pause/reprise, arrêt, rechargement après arrêt, maintien de la lecture après une ouverture de fichier invalide et remise à zéro après fin naturelle.

Ces essais vérifient le fonctionnement technique de la sortie. Ils ne remplacent pas une écoute subjective de qualité, ni des essais longs sur les fichiers de l'utilisateur.

### URL publique

La courte vidéo publique `https://www.youtube.com/watch?v=jNQXAC9IVRw` a été importée via le code Tonal, convertie par FFmpeg en WAV, chargée dans rodio puis arrêtée. Le test vérifie que le dossier temporaire est supprimé après arrêt. Les tests réseau utilisent uniquement des données factices ou publiques.

Pour une autre URL de test : définir `TONAL_TEST_YOUTUBE_URL`. Ce test reste optionnel et peut échouer si YouTube ou le contenu change.

### WebView2 temporaire

Deux cycles consécutifs ont créé une fenêtre privée sur un thread dédié, puis l'ont annulée automatiquement après trois secondes, sans interaction avec les identifiants Google. Les deux appels ont renvoyé « Connexion annulée. » et leurs threads se sont terminés.

Après ces essais, une vérification des processus `msedgewebview2.exe` dont la ligne de commande référençait `tonal-login-*` n'a trouvé aucun processus restant. Aucun dossier `tonal-login-*` ou `tonal-audio-*` issu des essais n'est resté dans le dossier temporaire.

Cela valide le cycle ouvert / annulé ; le cycle d'authentification réussie doit encore être testé manuellement.

### Session et métadonnées

- DPAPI : chiffrement/déchiffrement réussi ; altération du chiffré rejetée.
- Session : identifiants incomplets et injection d'en-têtes rejetés ; cookies inutiles ignorés.
- Navigation : HTTPS et domaines Google / YouTube explicites ; hôtes ressemblants et protocoles locaux rejetés.
- Innertube : signature SAPISIDHASH, version client, playlists, identifiants, titres, bibliothèque vide, continuation et changement de schéma testés sur données synthétiques.
- Requête réelle avec cookies **factices** : session rejetée. Depuis cette machine, la page Music sans consentement renvoie une redirection vers `consent.youtube.com`, que le client authentifié refuse sans transmettre ses en-têtes à cette autre origine.

## Validation encore nécessaire

Aucun compte Google réel n'a été utilisé par l'agent. La connexion, la bibliothèque authentifiée, la restauration après redémarrage, la pagination réelle, la révocation et les comptes secondaires / chaînes de marque ne sont pas validés.

Lors de la stabilisation initiale, l'outil de contrôle Windows était indisponible. La refonte suivante a ajouté une option de compilation `ui-capture` utilisant le mécanisme de capture interne d'eframe. Les captures de l'accueil en 1180 × 820 et 840 × 640, et de l'écran Fichiers locaux en format compact, ont été inspectées visuellement. Le noir absolu, les logos, les boutons et les espacements ont ainsi été vérifiés sur le rendu réel OpenGL. Les interactions et le contenu d'une bibliothèque réelle restent à valider par l'utilisateur.

## Vérification de la refonte UI

- Références visuelles : captures publiques de ytm-rs et Tubefast ; aucun code ni artwork de ces projets copié.
- Logos officiels SVG embarqués, sources documentées dans `assets/brands/README.md` ; Spotify reste non interactif avec SOON.
- `cargo test --locked` : 8 tests réussis, 4 essais d'environnement ignorés par défaut.
- `cargo test local_playback_smoke --locked -- --ignored --test-threads=1` : réussi après ajout de l'affichage de position/durée.
- `cargo clippy --all-targets --all-features --locked -- -D warnings` : réussi.
- Captures de contrôle : `target/ui-capture/home.png`, `compact.png`, `local.png`.
- Le filtre de bibliothèque concerne les métadonnées déjà chargées. La barre de progression est informative ; elle ne permet pas encore de déplacer la lecture.

Pour reproduire une capture sans automatiser le bureau :

```powershell
cargo build --features ui-capture --locked
$env:TONAL_CAPTURE_TO = "$PWD\target\ui-capture\home.png"
.\target\debug\tonal.exe
```

L'option `ui-capture` permet aussi `TONAL_CAPTURE_COMPACT=1` et `TONAL_CAPTURE_PAGE=menu` ou `settings`. Ces variables n'affectent pas la release standard. Le mécanisme de capture attend le rendu stabilisé, puis utilise la commande Screenshot d'egui ; l'application de contrôle se ferme après enregistrement.

La simplification suivante retire la barre latérale, l'écran Fichiers locaux et les deux blocs de source. Les captures `centered.png`, `centered-compact.png`, `account-menu.png` et `settings.png` servent à vérifier la nouvelle disposition et l'accès aux paramètres. Les captures plus anciennes documentent l'état précédent.

La barre Windows a ensuite été remplacée par un en-tête egui : marque, filtre de bibliothèque, menu compte et commandes natives minimiser / agrandir-restaurer / fermer. Les déplacements et redimensionnements passent par les commandes de viewport d'egui. Le rendu et le menu compte ont été inspectés en 1180 × 820 et 840 × 640 (`window-bar.png`, `window-bar-compact.png`, `window-account.png`). L'icône « t » a également été extraite de l'exécutable pour vérifier son intégration aux ressources Windows. Clippy avec toutes les options et les 8 tests automatiques passent ; les quatre essais d'environnement restent optionnels.

Les boutons et le menu compte ont ensuite été harmonisés (`refined-window.png`, `refined-menu.png`, `refined-settings.png`). Les trois démarrages de contrôle ont confirmé le succès de `DwmSetWindowAttribute` pour `DWMWA_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND`. La capture interne egui vérifie les composants ; elle ne capture pas le découpage des coins effectué ensuite par le compositeur Windows. L'opt-in natif est documenté par [Microsoft](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/ui/apply-rounded-corners). Clippy et les 8 tests automatiques restent réussis.

Pistes de bibliothèques consultées, sans migration effectuée : [egui-shadcn](https://github.com/FerrisMind/shadcn-rs) (la version examinée cible egui 0.33 ; Tonal utilise 0.32), [GPUI Kit](https://github.com/longbridge/gpui-kit) et [Iced](https://iced.rs/). Les deux dernières changeraient le framework de l'interface.

La passe suivante du lecteur est vérifiée dans `player-idle.png`, `player-playing.png` et `player-compact.png` : marges de 8 points entre le visuel et les trois bords de sa zone, titre Segoe UI Bold 18 points, artiste 13 points, commandes principales de 58 / 44 points et progression réelle sur toute la séparation. Un WAV silencieux généré de 20 secondes, placé à 8 secondes, sert de fixture ; les titres et artistes des captures sont explicitement des données de contrôle. Cette prévisualisation est limitée à `ui-capture` via `TONAL_CAPTURE_AUDIO`, `TONAL_CAPTURE_TITLE` et `TONAL_CAPTURE_ARTIST`.

Le titre et l'artiste de la bibliothèque accompagnent désormais le résultat d'import et sont appliqués au lecteur seulement après un chargement réussi. L'essai audio local vérifie aussi que l'artiste précédent reste intact après une ouverture invalide et est effacé après arrêt. La refonte fine du slider de volume reste reportée.

Aucun benchmark reproductible de RAM, CPU ou démarrage n'a été effectué. L'import reste une conversion WAV complète et non une lecture progressive.

## Lecteur : progression, volume et modes de lecture

La passe suivante est inspectée dans `player-controls.png`, `player-volume.png` et `player-volume-compact.png` : progression rouge, commandes sans cercle (zone principale 78 points, pictogramme 56), icônes Lucide à extrémités arrondies, durée à gauche du volume, slider blanc/gris avec poignée blanche de rayon 4,5 points, puis répétition et aléatoire. Les captures avec slider utilisent `TONAL_CAPTURE_VOLUME=1` dans la configuration de contrôle ; le déclenchement réel utilise la position du pointeur dans le coin inférieur droit et conserve l'ouverture pendant le glissement ou le focus clavier.

- `cargo test --locked` : 12 réussis, 4 essais d'environnement ignorés par défaut.
- `cargo clippy --all-targets --all-features --locked -- -D warnings` : réussi.
- `local_playback_smoke` relancé : WAV, FLAC, MP3 et OGG, y compris une répétition native après fin naturelle, avec titre, artiste et volume conservés.
- Quatre tests de file : fin sans répétition, bouclage playlist, répétition du titre distincte de suivant manuel, visite sans doublon en aléatoire, historique et indépendance du titre en cours pendant une préparation.
- La file est un instantané des titres déjà chargés. L'enchaînement réel d'une bibliothèque Google reste à valider avec un compte ; il utilise toujours l'import public complet existant. La répétition d'un titre réutilise le fichier temporaire sans nouveau téléchargement.
- Icônes : SVG officiels Lucide, adaptations et licence dans `assets/icons/`.
Ajustement suivant : pictogrammes volume / répétition / aléatoire réduits de 34 à 28 points, durée agrandie de 12 à 14 points. En-tête ramené de 72 à 58 points ; boutons de fenêtre carrés de 46 points avec marges supérieure et droite de 6 points. Le centre de la croix est donc à 29 points des deux bords. Captures natives inspectées : `compact-header.png` et `compact-header-volume.png`, en 1180 × 820 et 840 × 640. Clippy toutes configurations réussi.
## Correction du crash au clic sur Se connecter

Le journal Application Windows a enregistré le 11 novembre 2026 à 11:50 une violation d'accès `0xc0000005`, suivie de `0xc000041d`, dans `tonal.exe`. Le nouveau test `auth::tests::login_coexists_with_native_window` a reproduit `0xc000041d` avant correction.

Cause confirmée dans les dépendances locales : Winit 0.30.13 (eframe) et Tao 0.37.2 (connexion) enregistrent tous deux la classe Windows par défaut `Window Class`. Le deuxième enregistrement est ignoré ; la création de la fenêtre Tao appelle alors la procédure Winit avec des données incompatibles. Le test WebView2 isolé ne créait aucune fenêtre Winit et ne couvrait donc pas ce conflit.

Correction : classe explicite `Tonal.GoogleLogin.Tao` via `WindowBuilderExtWindows::with_window_classname`. Le test d'intégration crée d'abord la fenêtre egui, ouvre puis annule deux connexions sur leurs threads et vérifie le retour de l'interface. Il réussit après correction en debug, sans utiliser de compte ni lire d'identifiants. Aucun processus `msedgewebview2.exe` associé à `tonal-login-*` ne restait après ces cycles. Les 12 tests automatiques passent ; 5 essais d'environnement sont désormais ignorés par défaut. Clippy toutes options réussi.

Pour reproduire ce test avec une session Windows interactive :

```powershell
cargo test --locked login_coexists_with_native_window -- --ignored --test-threads=1
```

Cela valide l'ouverture et l'annulation en présence de l'interface ; l'authentification réelle et la récupération de bibliothèque restent à vérifier manuellement.
Le même test d'intégration réussit aussi avec `cargo test --release --locked login_coexists_with_native_window -- --ignored --test-threads=1` : deux cycles annulés, résultat 1 test réussi. La compilation release standard et Clippy toutes options passent après correction.

## Attente de validation Google et clés d'accès

La capture de l'utilisateur montre la page Google « Validation de votre identité » avec une demande de clé d'accès, avant le retour vers YouTube Music. Elle ne permet pas de conclure à un problème de récupération des playlists.

La connexion imposait InPrivate. Cette option est retirée : le profil reste neuf, isolé et temporaire, mais utilise le mode normal de WebView2. L'aide Google indique que certaines combinaisons système / navigateur ne permettent pas les clés d'accès en navigation privée : [aide Google](https://support.google.com/accounts/answer/13548313?hl=fr). C'est une cause possible, sans preuve que cela explique le blocage de cet utilisateur. Aucun choix de mot de passe n'est forcé et aucune protection Google n'est contournée.

Des messages de progression remontent maintenant du thread WebView2 : ouverture, validation d'identité Google, consentement, récupération de session, finalisation, puis vérification native et chargement des playlists. Ils sont calculés uniquement à partir du domaine et du chemin ; aucune URL complète, donnée de formulaire, clé ou mot de passe n'est remontée. L'annulation supprime aussi le canal de progression pour ignorer les retours tardifs.

Clippy toutes options et les 13 tests automatiques passent, avec 5 essais d'environnement ignorés par défaut. Le test des messages vérifie que mot de passe et clé d'accès partagent l'étape de validation, sans exposer les paramètres d'URL. Les essais sans identifiants ne peuvent pas valider une authentification Google réelle par clé d'accès ou mot de passe : ces deux parcours restent à essayer manuellement.
Le test d'intégration release a réussi après passage au profil temporaire normal : deux ouvertures et annulations avec l'interface native ouverte (7,14 secondes).

## Diagnostic du blocage persistant sur Google

Le passage au mode normal n'a pas résolu le problème réel : l'utilisateur signale une attente identique par mot de passe et clé d'accès, avec la fenêtre restant sur Google. La cause n'est pas encore établie.

La release de diagnostic écrit `tonal-login.log` à côté de l'exécutable (ici `target/release/tonal-login.log`). Le fichier est remplacé à chaque nouvelle tentative. Il contient les navigations autorisées/bloquées, demandes de fenêtres bloquées, chargements de page, étapes de lecture de session, heartbeat du thread et résultats booléens de disponibilité du navigateur. Aucun contenu de formulaire, cookie, titre, URL complète, chemin arbitraire, paramètre ou fragment n'est écrit. Les noms de domaines sont limités aux infrastructures Google/YouTube connues ; les autres sont remplacés par `external-site`.

Le heartbeat vérifie `document.readyState`, `navigator.onLine`, `window.isSecureContext` et l'existence de `PublicKeyCredential`, sans manipuler la connexion. Il ne prouve pas qu'une clé d'accès particulière fonctionne. Les changements de navigation ne sont pas élargis sans observation d'une redirection légitime manquante.

Validation : 14 tests réussis, 5 essais d'environnement ignorés ; Clippy toutes options réussi ; deux cycles de connexion annulés avec la fenêtre native ouverte réussis (7,70 secondes) ; compilation release réussie. Une tentative de l'utilisateur avec ce journal est nécessaire pour poursuivre le diagnostic.
## Redirection de transfert de session bloquée par Tonal

La tentative utilisateur du diagnostic montre à 19 secondes : `accounts.google.com` en transfert de session, puis `accounts.youtube.com`, puis `navigation-blocked site=external-site stage=session-transfer`. La page reste sur un challenge Google ; les heartbeats suivants confirment un document complet, en ligne et réactif. La navigation a donc été empêchée par la politique Tonal pendant le transfert, avant toute récupération native des playlists. Le domaine exact était masqué par la première version du diagnostic ; le mode privé n'était pas la cause observée.

Correction : embarquer les 187 domaines de la [liste officielle Google](https://www.google.com/supported_domains). Les domaines régionaux exacts et leurs préfixes `accounts.` / `www.` sont autorisés uniquement pour les chemins exacts `SetSID` et `CheckCookie` (avec ou sans `/accounts/`), en HTTPS sans identifiants d'URL ni port personnalisé. Les destinations globales précédentes restent autorisées. Aucun joker sur des suffixes inventés. Les diagnostics peuvent désormais nommer ces domaines régionaux connus sans conserver les paramètres des transferts.

Deux tests supplémentaires couvrent la chaîne globale → YouTube → Google régional → Music, et les rejets de domaines ressemblants, routes arbitraires, mauvais protocoles, ports et identifiants d'URL. Un transfert encore refusé interrompt maintenant la tentative avec une erreur explicite plutôt que d'attendre dix minutes. La trace initiale expurgée est conservée dans `target/ui-capture/blocked-session-transfer.log`.

Clippy toutes options et 16 tests automatiques réussis (5 essais d'environnement ignorés). Un nouvel essai réel permettra de confirmer la destination et le passage jusqu'à la bibliothèque ; le succès Google complet n'est pas encore revendiqué.
## Validation réelle après correction du transfert régional

L'utilisateur confirme « La bibliothèque s'affiche ». La nouvelle trace identifie précisément le transfert : à 33 secondes, `accounts.google.com` → `accounts.youtube.com` → **`accounts.google.fr`**, désormais autorisé, puis `music.youtube.com`. À 34 secondes : lecture de session terminée, session prête, boucle terminée, données de navigation effacées et fenêtre fermée. Aucun processus `msedgewebview2.exe` avec profil `tonal-login-*` ne restait après ce succès ; Tonal répond toujours.

Cela confirme que le domaine régional manquant causait le blocage observé. La trace passe par le challenge de mot de passe ; le parcours clé d'accès ne doit pas être déclaré testé sur cette base. La pagination réelle, la restauration après redémarrage et la lecture des titres restent à vérifier séparément. Le fichier chiffré de session a été contrôlé uniquement par ses métadonnées, sans lire ni afficher de secrets.
## Progression interactive, pochettes et compte Google

Implémentation : petit point rouge au survol de la progression, clic pour chercher et glissement avec prévisualisation de position puis déplacement au relâchement. Les commandes clavier du slider restent disponibles. La lecture en pause reste en pause après un déplacement.

Les métadonnées comprennent désormais la pochette de playlist, la pochette de morceau, l'album et la durée disponible. Les lignes sont cliquables, avec pochette 48 points, titre en gras, artiste / album puis durée à droite. La pochette du titre accompagne le résultat d'import et est remise à zéro à l'arrêt. Les playlists système LM / SE / HL / WL et les mix RD sont masqués par identifiant. Le bloc visuel YouTube Music / Spotify a été retiré ; Spotify reste désactivé.

Le prénom (premier élément du nom de compte) et l'avatar viennent de la réponse native `account/account_menu` déjà utilisée pour valider la session. Ils sont affichés dans le bouton et le menu Compte, sans appel à une API de profil supplémentaire. Le profil est effacé de l'état à la déconnexion.

Le cache d'images natif limite à quatre requêtes et 64 entrées, sans cookies ni en-têtes de session. Hôtes d'images publics YouTube / Google autorisés explicitement ; redirections refusées, réponse limitée à 4 Mio, dimensions de décodage limitées à 2048, allocation à 16 Mio et texture à 512 points sans agrandir les petites images. PNG / JPEG / WebP sont pris en charge. Le chargement est effectué hors du thread de rendu et limité aux éléments visibles ; une image neutre reste disponible en cas d'absence ou d'échec.

Vérification :

- `cargo clippy --all-targets --all-features --locked -- -D warnings` réussi.
- `cargo test --locked` : 20 réussis, 6 essais d'environnement ignorés.
- `seeking_preserves_pause_and_track_metadata` réussi sur WAV silencieux et sortie Windows : déplacement avant/arrière, pause maintenue, reprise et métadonnées conservées.
- Un test d'événements egui vérifie le clic, le glissement hors limites et le relâchement du slider.
- Les fixtures vérifient pochettes, album, durée, profil, playlists système masquées et absence de données inventées.
- Captures natives inspectées : `account-photo.png`, `playlist-artwork.png`, `seek-hover.png`. Le menu et les pochettes ont été vérifiés sur la bibliothèque réelle restaurée depuis DPAPI, sans utiliser les identifiants de connexion. La liste de morceaux réelle a été chargée, sans lecture automatique.

`TONAL_CAPTURE_WAIT_MS` laisse les métadonnées et images arriver avant la capture ; `TONAL_CAPTURE_PAGE=playlist` ouvre la première playlist pour le contrôle. `TONAL_CAPTURE_SEEK_HOVER=1` injecte uniquement dans la compilation de contrôle un mouvement du pointeur au-dessus de la progression. Le WAV et les métadonnées du lecteur de la capture `seek-hover.png` sont des données de démonstration.
## Survol des playlists, titre actif et pagination automatique

Les compteurs « titres chargés », le message « Lecture démarrée » et le bouton « Plus de titres » ont été retirés de la release. Les messages de succès de métadonnées ne sont plus affichés ; les erreurs restent visibles. Une carte de playlist au survol affiche Play en bas à droite, sans bordure ni disque blanc, sur un dégradé noir local progressif avec transition de 160 ms.

Le titre courant a un fond gris plus clair que le survol, une pochette assombrie et Pause tant que le son joue. En pause, l'icône devient Play. Cliquer cette ligne bascule la pause sans nouvel import. L'identification repose sur le titre de la file et la présence d'une source audio, sans comparer les noms affichés.

Les pages sont demandées séquentiellement lorsque le bas de la liste approche de la zone visible (anticipation de 400 points). Les lignes hors écran réservent leur espace sans construire leurs textes ni charger leurs images. Les titres reçus prolongent la file du même contexte de playlist en conservant le titre et l'historique, y compris si une préparation audio était en cours. Un échec bloque les tentatives automatiques jusqu'à « Réessayer le chargement » ; une continuation identique à celle demandée provoque une erreur explicite pour éviter une boucle.

Validation : Clippy toutes options réussi, 21 tests automatiques réussis (6 essais d'environnement ignorés), dont un nouveau cas d'extension de file sans doublon de page ni changement de titre courant. Captures inspectées : `playlist-hover-play.png`, `active-track.png`, `automatic-track-pages.png`. Le contrôle au bas de la playlist réelle a chargé progressivement 611 titres et terminé sans continuation. Le rendu du titre actif utilise le WAV silencieux de démonstration avec les métadonnées du deuxième morceau ; aucun titre YouTube n'a été joué automatiquement pour ce contrôle. Les modes de capture et leurs compteurs de diagnostic restent exclus de la release standard.
## Total de playlist et infobulles

Le total « n titres » est rétabli sous le nom de la playlist à partir du compteur fourni dans les métadonnées de bibliothèque. Il reste indépendant des titres de la page courante. Les séparateurs de milliers, espaces insécables et libellés français / anglais sont pris en charge ; les nombres abrégés ou textes non reconnus ne sont pas transformés en faux totaux. En absence de compteur, la longueur de la liste n'est utilisée qu'après chargement complet et sans erreur.

Le rendu natif `playlist-total.png` confirme « 611 titres » alors que le contrôle a chargé 100 titres avec une continuation. Le nouveau test couvre cette extraction et les valeurs manquantes : 22 tests réussis, 6 essais d'environnement ignorés ; Clippy toutes options réussi. Les infobulles explicites et celles qu'egui ajoute automatiquement aux labels tronqués ont été désactivées pour playlists, titres et artistes. Les aides des commandes sont conservées.

Le délai de changement de morceau a été expliqué d'après le code : `request_queue` attend `PublicStreamResolver::prepare`, qui lance yt-dlp, télécharge tout le meilleur flux puis attend sa conversion WAV par FFmpeg avant de charger rodio. Le fichier temporaire du titre précédent est libéré lors du remplacement. Aucun préchargement du prochain titre ni cache audio persistant n'est implémenté ; les changements manuels doivent donc attendre toute cette préparation. Aucune mesure de durée par étape n'a été réalisée et aucune optimisation du pipeline audio n'est revendiquée dans cette passe.
## Streaming natif, Premium et sélection immédiate — 11 novembre 2026

Cette passe remplace le téléchargement complet et la conversion WAV décrits dans les sections historiques ci-dessus. `youtube.rs` résout uniquement le flux par yt-dlp ; `native_stream.rs` transporte les blocs HTTP et décode Opus / AAC sur un worker. rodio reçoit une file PCM bornée sans lecture réseau sur le thread de sortie. Le transport épingle les deux premiers blocs et conserve au maximum 32 blocs de 256 Kio (8 Mio). Le préchargement du prochain morceau récupère au plus deux blocs, sans décoder ou lire ce morceau à l'avance. Aucun FFmpeg ni fichier musical dans le chemin de lecture YouTube.

La résolution utilise la session DPAPI existante par une configuration transmise sur stdin, sans cookies dans les arguments ou fichiers temporaires. Les clients par défaut et Web Music sont interrogés ; la sélection privilégie Opus 774, AAC 141, Opus 251, AAC 140, puis un autre flux audio compatible. YouTube décide des formats accessibles ; Tonal ne prétend pas qu'un compte est Premium à partir de la simple présence de cookies. Les téléchargements CDN sont limités à Google Video en HTTPS, redirections comprises, sans cookies. Les erreurs yt-dlp et URL signées ne sont jamais reproduites dans l'interface ou les journaux.

Contrôles réalisés :

- `cargo test --all-features --locked` : **25 réussis**, 9 essais d'environnement ignorés par défaut ; Clippy toutes cibles, avec et sans toutes les options, réussi. Le test supplémentaire contrôle le changement de fréquence et de nombre de canaux exactement à la frontière des blocs PCM pour rodio.
- `compressed_cache_is_bounded_and_preload_is_half_a_megabyte` : serveur HTTP local, préchargement exactement 512 Kio, lecture de 40 blocs différents, cache conservé à 32 blocs / 8 Mio, en-tête conservé et annulation effective.
- `progressive_codecs_seek_and_replay` : fichiers synthétiques AAC 256 et Opus 256 de 45 secondes, servis par blocs HTTP ; premiers échantillons non silencieux obtenus avant téléchargement complet, déplacements à 25 s puis 4 s, fin et reprise à zéro via la file PCM non bloquante. Réussi pour les deux codecs.
- `authenticated_native_stream_smoke` : session Google enregistrée, flux réel AAC autour de **258 kbit/s mesurés** (palier nominal 256), résolution et premier préchargement environ **7,23 s**, puis position audio supérieure à 100 ms après environ **141 ms**. Pause conservée pendant déplacement à 10 s, reprise et déplacement arrière à 2 s réussis. Ce débit et ces délais sont ceux d'un essai, pas une garantie ni un benchmark général.
- `public_native_stream_smoke` : aucune session fournie, flux réel **Opus 251 autour de 106 kbit/s** sur la même courte vidéo ; résolution et préchargement environ **4,38 s**, puis consommation audio confirmée après environ **121 ms** ; pause et déplacements réussis. Le débit Opus varie selon le contenu ; il s'agit bien du palier standard.
- `immediate_selection_cancels_previous_and_uses_prefetched_audio` : première vraie playlist du compte, volume coupé. Les deux premiers clics affichent immédiatement leurs métadonnées alors que la source audio est vide ; le second annule le token du premier. Après résolution du second morceau et préchargement du troisième, « Suivant » consomme le résultat déjà préparé au prochain poll, environ **0,18 ms** dans cet essai (transition d'état, pas délai acoustique). Arrêt efface la sélection et annule les tâches restantes. Réussi.
- Capture native inspectée `target/ui-capture/pending-native-track.png` : le premier titre de la playlist, son artiste et sa durée sont déjà dans le lecteur avec l'indicateur de chargement ; diagnostic de capture `pending=true, selected=true, audio_loaded=false`. Les pochettes non mises en cache se chargent ensuite ; la sélection n'attend pas leur téléchargement.

Les essais réseau ont tous été lancés explicitement avec `--ignored --nocapture` ; ils ne divulguent ni nom de compte, ni cookies, ni identifiants de bibliothèque, ni adresses signées. L'application de capture quitte et annule sa préparation audio après l'image. La release standard exclut les variables de contrôle.

Restent non vérifiés : renouvellement d'une adresse réellement expirée, pertes réseau prolongées, parcours de tous les codecs / contenus restreints et mesures complètes RAM / CPU. La résolution initiale dépend encore de yt-dlp et peut prendre plusieurs secondes ; le préchargement ne garantit pas une lecture immédiate d'un morceau choisi au hasard. Les parcours de connexion WebView2 ne sont pas modifiés par cette passe.

## Paramètres, abonnement et badges de qualité — 11 novembre 2026

Le dialogue de paramètres est désormais un `egui::Modal` : il bloque les entrées vers la bibliothèque, conserve le fond noir et ferme par la croix, le clic sur le fond ou Échap. Le flou est calculé sur le framebuffer de l'interface déjà dessinée, avant le dialogue, avec deux passes gaussiennes OpenGL à un tiers de la résolution de la fenêtre. Les textures sont réallouées si la taille change et les ressources sont détruites à la fermeture de l'application. Le shader n'est exécuté que pendant l'ouverture des paramètres ; en cas de compilation impossible, le fond reste assombri et les réglages utilisables. Aucun navigateur n'est impliqué.

Les paramètres affichent la photo et le nom du compte, le statut `IS_SUBSCRIBER` extrait de la page Music authentifiée et la connexion Spotify désactivée avec SOON. Le volume et l'apparence ont été retirés. Le statut est `Option<bool>` : vrai = Premium, faux = Standard, absent = non vérifié. Le compte n'est jamais classé à partir de la présence de cookies ou d'un morceau standard. L'essai expurgé `subscription_status_live`, lancé explicitement, a confirmé le statut Premium du compte enregistré sans afficher son identité. Le parseur est couvert pour vrai, faux, champ absent et chaîne de caractères invalide.

Le badge du lecteur suit le débit réel : S blanc en dessous de 160 kbit/s, H vert de 160 à moins de 250, M doré à partir de 250. Le choix remplit la plage 150–160 laissée entre les seuils demandés ; 250 appartient à M. Un débit inconnu ou invalide ne produit pas de badge. Les formats Premium 141 / 774 peuvent utiliser leur débit nominal 256 si le résolveur ne fournit pas de mesure. Ces catégories Tonal ne certifient pas un master studio ou une source lossless. Les seuils, décimales et valeurs absentes / invalides sont couverts par un test. La préférence d'affichage est atomiquement enregistrée dans un fichier JSON d'interface distinct du stockage de session DPAPI.

Les icônes Lucide sont rasterisées à deux fois leur taille physique selon le facteur DPI, puis réduites avec des mipmaps linéaires. Cela évite le sous-échantillonnage d'une texture fixe de 128 pixels sans filtre adapté, qui produisait des contours irréguliers sur les petits dessins de volume, répétition, aléatoire et recherche.

Validation : 27 tests automatiques réussis, 10 essais d'environnement ignorés par défaut. Les contrôles de capture de `ui-capture` injectent les clics egui vers les rectangles effectivement affichés : `close`, `outside` et `escape` ont chacun produit `settings=false`. Deux démarrages successifs avec l'action `toggle` ont produit `show_quality=false` puis `true`, confirmant l'application et la conservation du réglage entre processus ; l'état final est activé. Ces contrôles restent exclus de la release standard.

Captures inspectées : `settings-redesign.png`, `settings-redesign-compact.png` (840 × 640, sans débordement), `settings-action-close.png`, `quality-badge-200.png` ; les rendus des trois débits sont aussi disponibles dans `quality-badge-140.png`, `quality-badge-200.png`, `quality-badge-258.png`. Le lecteur de ces captures utilise le WAV silencieux de contrôle avec des métadonnées et débits synthétiques, sans prétendre à une mesure de qualité de ce WAV. Le compte et le statut, eux, proviennent de la session enregistrée.

Les badges de débit S / H / M ont ensuite été réduits de 20 à 16 points, avec coins de 2 points. Les lettres sont dessinées par tracés vectoriels anticrénelés (courbes de Bézier pour S, segments pour H / M), et le fond est aligné sur les pixels physiques ; elles ne dépendent plus d'une petite glyphe d'atlas de police. L'espace réservé à côté du titre suit maintenant la taille réelle du badge et l'espacement du layout. Captures inspectées : `small-vector-badges-settings.png`, `small-vector-badge-player.png`.

Le statut Premium / Standard / non vérifié est maintenant placé dans la même ligne que le prénom, à 8 points de celui-ci, avec troncature du prénom si nécessaire pour préserver le badge. Le bouton de déconnexion reste à droite. Le rendu compact est vérifié dans `premium-next-to-name.png`. Clippy toutes cibles et toutes options est réussi ; aucune logique de lecture, de seuil ou de compte n'est modifiée dans cette passe visuelle.

Après déplacement du badge d'abonnement, le bouton de déconnexion est recentré verticalement dans la carte du compte, à la hauteur de l'avatar. Sa zone cliquable de 32 × 32 points conserve une marge de 16 points à droite, avec 8 points de séparation de la zone de texte. Capture compacte inspectée : `account-logout-centered.png` ; Clippy toutes cibles et toutes options réussi. La déconnexion elle-même n'a pas été déclenchée pendant ce contrôle visuel pour conserver la session de l'utilisateur.

À la demande de l'utilisateur, les lettres S / H / M ont retrouvé Segoe UI Bold 12 points, plutôt que les tracés fins. Leur centre visible est aligné dans le fond, et le carré est maintenant réduit à 14 points pour diminuer la marge intérieure sans réduire les lettres. Dans le lecteur, le centre vertical du badge est calculé sur les limites visibles des glyphes du titre, plutôt que sur la hauteur de ligne ; le fond et la position des lettres sont alignés sur les pixels physiques. L'allocation horizontale suit cette nouvelle taille. Captures inspectées : `compact-bold-badge-player.png` et `compact-bold-badges-settings.png`. Clippy toutes cibles et toutes options réussi ; aucune logique audio modifiée.

Le centrage vertical précédent ne correspondait pas à la demande de l'utilisateur : le badge doit reposer sur la ligne de base du titre. Son bord inférieur est désormais ancré sur la position de baseline des glyphes du titre. La zone de survol utilise aussi le rectangle réellement dessiné. Le carré reste de 14 points ; le M est une capitale géométrique avec traits gras de 2 points et contreformes ouvertes, pour éviter le dessin écrasé de la petite glyphe. S et H utilisent une police classique en gras, rendue à trois fois sa taille puis réduite, avec repli sur la police existante si Arial Bold est absent.

La capture `badge-title-baseline.png` reprend « Ain't No Sunshine » / « Lighthouse Family » de l'exemple utilisateur avec la fixture audio silencieuse. Le contrôle des pixels confirme le bas du titre et le bas du carré tous deux sur la ligne 767 à 100 % de DPI. Les lettres sont aussi vérifiées dans `badge-bold-shapes.png`. Clippy toutes cibles et toutes options réussi. Aucun seuil de qualité ni comportement audio modifié.

## egui 0.37 et aléatoire sur toute la playlist — 12 novembre 2026

La migration egui / eframe / egui_extras 0.32.3 → 0.37.0 est terminée. Le nouveau moteur Skrifa / Vello CPU utilise son hinting natif pour les lettres S / H / M de la même police en gras ; les capitales dessinées à la main et le suréchantillonnage de glyphes ont été retirés. Le carré de 14 points conserve son ancrage sur la ligne de base du titre. `brands.rs` remplace les deux textures de marque fixes à 128 pixels par un cache SVG par marque / taille physique / DPI, rendu à 2× puis filtré avec mipmaps, en conservant les proportions. L'interface reste egui / OpenGL et les mêmes assets officiels sont utilisés.

Les panneaux, les identifiants, les rôles accessibles et les captures ont été portés aux API 0.37. Les captures passent par ScreenshotCallback ; le test de déplacement sans renderer libère explicitement ses deltas de textures selon le nouveau contrat. L'état audio et les tâches sont traités dans `App::logic`, y compris quand la fenêtre est masquée. Les contrôles du lecteur ont des identifiants explicites stables : l'apparition du slider de volume ne change plus l'identité du bouton aléatoire pendant le premier clic.

Contrôles de migration réalisés : 27 tests automatiques réussis avant ajout du correctif shuffle ; Clippy toutes cibles / toutes options et sans options réussi. `auth::tests::login_coexists_with_native_window`, exécuté explicitement, a ouvert puis annulé deux connexions WebView2 avec la fenêtre egui 0.37 active, sans identifiants, et a réussi. Les trois fermetures des paramètres (croix, clic extérieur, Échap) et deux bascules successives de préférence ont été vérifiées avec les captures de contrôle. Le réglage de qualité est conservé entre processus ; les contrôles finaux ne le modifient pas.

Captures inspectées : `egui037-settings.png`, `egui037-action-close.png`, `egui037-settings-150.png`, `egui037-player-150.png`, `egui037-playlist.png`. Le facteur 150 % est simulé par le zoom egui et une fenêtre physique agrandie pour conserver la taille logique. Le bas du badge et le bas des lettres de l'exemple « Ain't No Sunshine » restent sur la ligne 767 à 100 %. Les textes et l'audio des captures utilisent la fixture silencieuse déjà documentée ; les métadonnées de bibliothèque et de compte sont réelles.

Le mode aléatoire prépare désormais le catalogue complet de la playlist en cours, indépendamment des pages visibles. Un worker HTTP distinct récupère les pages séquentiellement avec le même provider et la même session, sans charger les images ni l'audio. Le morceau courant continue. Le prochain tirage et le préchargement attendent la fin du catalogue : ils ne sélectionnent pas silencieusement dans une liste partielle. Un petit indicateur sur Aléatoire signale la préparation ; Suivant redevient disponible une fois prête et permet une nouvelle tentative en cas d'erreur. Le cache en mémoire ne conserve qu'un catalogue, invalidé à la réouverture / actualisation ; la pagination de l'affichage reste progressive.

La file distingue maintenant l'historique réellement parcouru des indices simplement placés avant la sélection initiale. Activer Shuffle au milieu de la playlist inclut donc aussi les titres antérieurs non joués. La réception du catalogue réconcilie les occurrences de vidéo, conserve les doublons et l'historique, et maintient le morceau courant même s'il a été retiré de la playlist entre-temps. Arrêt, déconnexion, changement de playlist de lecture et désactivation d'Aléatoire annulent le worker ; naviguer dans une autre playlist affichée ne remplace pas le contexte de lecture. Les erreurs n'entraînent pas une boucle de requêtes automatiques. La collecte est bornée à 50 000 entrées, 32 Mio de textes et 1 000 continuations distinctes ; une continuation cyclique est refusée.

Validation finale du correctif : **32 tests automatiques réussis**, 11 essais d'environnement ignorés par défaut ; Clippy toutes cibles, avec et sans toutes les options, réussi. Nouveaux cas : sept pages de 611 titres, passage d'une file de 100 à 611 sans rejouer l'historique, activation après une sélection en milieu de liste, vidéo courante retirée, doublons, annulation, erreur réseau et pagination cyclique.

L'essai réel `shuffle_reaches_the_full_real_playlist_while_only_one_page_is_visible` a réussi avec **100 titres visibles et 611 entrées dans le catalogue**. Il vérifie chaque occurrence une fois, le report de l'avance pendant la préparation, la réutilisation du cache lors d'une nouvelle sélection et l'arrêt des tâches, sans jouer de musique. L'essai existant `immediate_selection_cancels_previous_and_uses_prefetched_audio` a également réussi, volume coupé ; transition d'état vers le titre préchargé observée à environ 0,19 ms (pas un délai acoustique).

La capture `shuffle-full-playlist.png` utilise le vrai bouton Aléatoire : pointeur, appui puis relâchement injectés sur son rectangle réel, avec un WAV silencieux en pause. Le diagnostic final indique `tracks=100`, `continuation=true`, `shuffle=true`, `complete=true`. Aucun texte de compteur technique ni message de succès supplémentaire n'est ajouté au produit. L'extension automatique de la file séquentielle sans défilement reste distincte de ce correctif.

## Survol du texte et pause réseau — 12 novembre 2026

La sélection des labels est désactivée globalement par `style.interaction.selectable_labels = false`. Le champ de recherche conserve son édition et sa sélection normales ; sa hauteur et son alignement vertical sont explicitement définis. La capture native `target/ui-capture/track-text-hover.png` place le pointeur sur les lettres du premier titre d'une vraie playlist. Elle vérifie que la ligne reste survolée et que le curseur reste une main, puis montre le fond de survol et Play sur la pochette. L'alignement de la recherche sur la loupe a aussi été inspecté.

Le défaut identifié était une pause limitée à la sortie audio : le worker de décodage et le préchargement pouvaient continuer. `http_audio.rs` utilise maintenant reqwest asynchrone sur un runtime Tokio partagé à un worker. Une pause interrompt les requêtes actives, libère leur pool de connexions et bloque les suivantes ; un token distinct interrompt aussi un déplacement devenu obsolète ou le renouvellement d'URL yt-dlp. Le décodage attend la reprise, qui repart de la position conservée avec le même cache compressé. Le préchargement du prochain titre est annulé par AppState et ne redémarre pas pendant la pause. Les requêtes de métadonnées de bibliothèque restent indépendantes.

**34 tests automatiques réussis**, 11 essais d'environnement ignorés par défaut. Deux nouveaux tests HTTP locaux vérifient une réponse audio partielle interrompue : retour en moins de 500 ms et fermeture TCP constatée par le serveur, aussi bien à la pause qu'à l'annulation du préchargement. Ils couvrent le refus de requête pendant la pause et une requête réussie après reprise. Clippy toutes cibles / toutes options réussi.

L'essai explicite `progressive_codecs_seek_and_replay` réussit avec Opus et AAC : position et octets transférés stables pendant la pause, déplacement à 25 secondes sans téléchargement, reprise puis fin et relecture. L'essai réel `immediate_selection_cancels_previous_and_uses_prefetched_audio` réussit, volume coupé, avec annulation du préchargement à la pause, absence de redémarrage pendant plusieurs polls et nouveau préchargement à la reprise. Les essais réels `public_native_stream_smoke` et `authenticated_native_stream_smoke` réussissent également : Opus 251 à environ 106 kbit/s et AAC 141 à environ 258 kbit/s, avec déplacement pendant la pause puis reprise.

La disparition de l'avertissement de lecture simultanée dans le navigateur reste à confirmer par l'utilisateur : ces essais prouvent l'arrêt des requêtes de Tonal, pas le délai de libération de session appliqué par YouTube.

## Reprise après une nuit et pochettes en échec — 12 novembre 2026

Trois défauts ont été constatés dans le code : l'état `Failed` des images empêchait définitivement toute nouvelle tentative ; le renouvellement audio était limité à une seule fois pendant toute la vie d'un flux ; une erreur de décodage arrêtait et vidait le lecteur, rendant son bouton Lecture inutilisable. Cela fournit des causes plausibles aux symptômes signalés, sans établir rétrospectivement quel événement réseau ou Windows s'est produit pendant la nuit de l'utilisateur.

Les pochettes visibles sont maintenant retentées avec un délai exponentiel de 2 secondes à 2 minutes, en conservant les limites de 4 workers et 64 entrées. Une texture prête n'est pas effacée par un autre échec. Le test local `a_transient_failure_is_retried_without_dropping_ready_artwork` utilise un vrai serveur TCP qui répond d'abord HTTP 503, puis fournit un PNG : il vérifie l'absence de nouvelle requête avant le délai, la réussite au nouvel essai et la conservation d'une autre texture déjà prête. Seul son helper privé permet la fixture HTTP locale ; la validation des hôtes HTTPS publics reste appliquée dans `texture()`.

Les liens audio sont renouvelés avant leur expiration connue (marge de 60 secondes) et après refus HTTP 403 / 410, une seule fois par lecture de bloc. Le format et la taille doivent rester compatibles avec le cache. La limite ne s'applique plus à toute la vie du morceau : `the_same_stream_can_renew_after_multiple_expirations`, exécuté explicitement, force deux expirations successives du même flux et réussit deux véritables résolutions / préchargements, sans sortie audio. Un test déterministe couvre la marge, une expiration passée et les dates absentes / invalides.

La sortie rodio utilise un callback d'erreur CPAL ; une erreur réseau / décodage / sortie conserve le titre et sa position en pause. Lecture relance une préparation annulable à cet endroit ; un échec de préparation laisse aussi Lecture disponible pour réessayer. Après une pause d'au moins 30 minutes, une reprise ou une nouvelle sélection réouvre la sortie Windows, conserve le volume et demande un flux frais. La file et son historique restent en place. Aucune récupération audio n'est lancée pendant la pause et aucune boucle automatique de reprise n'a été ajoutée.

`overnight_and_output_failure_recover_the_current_track_at_its_position`, exécuté explicitement, utilise la session enregistrée et la sortie Windows, volume coupé. Il simule une pause de huit heures, vérifie le retour du même morceau au-delà de sa position de 5 secondes, puis injecte le signal d'erreur de sortie et vérifie une seconde reprise au même endroit, avec la même file et le même volume. Cet essai simule le temps et l'erreur : il ne met pas Windows réellement en veille et ne débranche pas de périphérique.

**36 tests automatiques réussis**, 13 essais d'environnement ignorés par défaut ; Clippy toutes cibles / toutes options réussi. Une nuit réelle et les conditions exactes de disparition de la pochette restent à confirmer par l'utilisateur. Un lien d'image définitivement révoqué peut nécessiter une actualisation des métadonnées de bibliothèque ; les nouvelles tentatives corrigent les échecs transitoires, pas une URL définitivement invalide.

## Résolution, lecture continue, reprise, Windows et file — 12 novembre 2026

Le périmètre retenu avec l'utilisateur comprend l'accélération de résolution, la lecture séquentielle sans défilement, la reprise au redémarrage, les commandes Windows et une file temporaire au centre. La recherche reste dans la bibliothèque de l'utilisateur. Suggestions liées aux écoutes, normalisation facultative et mini-lecteur sont conservés pour la suite ; pas de recherche globale ni de paroles.

`direct_audio.rs` essaie les URL exploitables de l'API player, avec le contexte de version / visiteur obtenu par le provider. Les en-têtes de session restent sur l'origine YouTube Music ; le test d'une URL CDN ne transmet aucun cookie, et les formats non autorisés ou chiffrés passent à yt-dlp. Le chemin authentifié ne remplace pas silencieusement un format Premium par un format standard pour gagner du temps. Les réponses sont bornées, les requêtes annulables, les hôtes CDN et images contrôlés, et les refus provoquent un court délai avant une nouvelle tentative native.

Limite constatée : les deux morceaux réels de bibliothèque examinés renvoient leurs formats via `signatureCipher`. Leur déchiffrement utilise toujours yt-dlp ; le chemin entièrement HTTP n'a donc pas été déclaré fonctionnel sur ces échantillons. Le repli essaie désormais Music seul, puis les clients habituels si nécessaire. Un refus connu de l'API Music évite cet essai spécialisé pour un morceau non musical. Le budget des deux étapes yt-dlp reste borné ; aucune configuration ou donnée de session n'est placée dans ses arguments.

L'essai explicite `authenticated_resolver_benchmark` compare les deux modes sur le même morceau et vérifie le préchargement réel : **Opus 774 à environ 261 kbit/s** dans les deux cas, environ **7,13 s** de résolution multi-client contre **4,73 s** avec Music seul (7,19 / 4,79 s avec les premiers blocs audio). Il s'agit d'une paire de mesures, dans cet ordre, sans contrôle statistique du cache système ni du réseau ; aucune latence universelle ou lecture instantanée n'en est déduite.

Le nouveau format Opus Premium a révélé un panic du décodeur expérimental `opus-decoder 0.1.1` dans `celt/vq.rs` lors d'un contrôle de lecture. Il est remplacé par `opus 0.4.0` / `opusic-sys 0.7.7` et libopus embarqué. Le flux compressé Premium reste celui de YouTube : aucun réencodage, WAV ou FFmpeg n'entre dans la lecture. CMake portable a été installé uniquement dans `target/build-tools/`, depuis la distribution officielle, avec comparaison SHA-256 ; `scripts/run.ps1` le détecte. La licence libopus est conservée dans `assets/licenses/libopus.txt`.

La lecture séquentielle déclenche la récupération paginée du catalogue quand cinq titres connus ou moins restent à parcourir. Les titres déjà connus continuent sans attendre ; en limite de page, Suivant attend le résultat. La réception complète conserve l'ordre séquentiel et n'ajoute pas les anciens indices avant le morceau sélectionné à la fin de la file. `restored_paused_queue_continues_sequentially_past_the_visible_page`, exécuté explicitement, vérifie **100 titres visibles et 611 jouables**, une reprise injectée à 43 secondes en pause, puis chaque titre de l'indice 96 à 610, sans défilement ni lecture audio.

`playback_store.rs` écrit les métadonnées de reprise sur un worker dédié, par remplacement atomique et DPAPI, avec une file de sauvegarde bornée. La reprise est liée à la session, sa structure et ses dimensions sont validées, et aucun lien audio signé ni cookie n'est enregistré. Arrêt / Déconnexion effacent la reprise. Le test automatique couvre chiffrement, ordre aléatoire, historique, entrée retirée, position, volume, autre session et structure invalide. Un cycle de deux vrais processus, sur `target/ui-capture/restart-qa.dpapi`, a sauvegardé puis restauré le même titre à environ **13,2 secondes**, en pause, sans préparation audio (`audio_loaded=false`, `pending=false`). Les captures de contrôle n'utilisent jamais le fichier de reprise normal de l'utilisateur.

Le module `desktop.rs` publie SMTC via souvlaki et reçoit les événements sur le fil d'état egui. `examples/media_probe.rs` utilise la session Windows réelle de Tonal et a vérifié métadonnées, Pause, déplacement à 3 secondes, Lecture et Suivant. Un premier contrôle a montré qu'un chargement annoncé à Windows comme déjà en pause empêchait la livraison d'une nouvelle commande Pause ; l'état publié représente désormais l'intention de lecture. La seconde vérification réussit aussi une Pause pendant la préparation du morceau suivant, toujours appliquée sept secondes plus tard. Le décodeur et le HTTP peuvent être créés directement suspendus. Un casque physique n'a pas été manipulé pendant ces essais.

La zone de notification est optionnelle et désactivée par défaut. L'essai `TONAL_CAPTURE_ACTION=tray` crée la vraie icône, masque la fenêtre, constate `IsWindowVisible=false`, puis fait repasser une action Afficher par le même canal de réception que l'icône ; la fenêtre redevient visible et répond. Le déclenchement du menu Shell par un clic physique n'est pas simulé. Quitter et la fermeture des captures contournent explicitement la réduction pour permettre l'arrêt du processus.

Le panneau `À suivre` s'ouvre avec la petite icône située à gauche de Précédent. Il est ancré au-dessus du lecteur et se ferme par croix / extérieur / Échap. Ses titres ne sont pas sélectionnables ; les suivants sont déplaçables par leur poignée, jouables immédiatement ou retirables via `…`. Les suppressions sont mémorisées par occurrence pour rester supprimées après pagination ou activation du shuffle ; les playlists YouTube ne sont jamais modifiées. Un test de file couvre déplacement, suppression, extension, shuffle, lecture immédiate et historique. Un glissement réel injecté dans l'interface a changé les deux premiers suivants en conservant le morceau courant ; la fermeture par la croix a aussi été vérifiée. Captures inspectées : `queue-drag.png`, `queue-closed.png`, `queue-panel-compact.png`, `restart-restored.png` et `tray-roundtrip.png`, sous `target/ui-capture/`.

Contrôles Rust finaux : **40 tests automatiques réussis**, 16 essais d'environnement ignorés par défaut ; Clippy toutes cibles, avec et sans toutes les options, réussi. Après remplacement du décodeur, les essais explicites Opus/AAC progressifs avec fin / déplacement / reprise, sélection immédiate / annulation / préchargement et lectures réelles publique / authentifiée ont réussi. Les préférences de qualité de l'utilisateur et son fichier de session ont été conservés.

## Aléatoire intelligent et normalisation facultative — 12 novembre 2026

Deux réglages persistants, désactivés par défaut, sont ajoutés sous Lecture. L'aléatoire intelligent intervient lorsque le shuffle est actif : il réordonne les suivants avec une pénalité pour les 20 vidéos récentes et les deux artistes précédents, sans exclure de titre. Les artistes sont comparés sur leur libellé normalisé ; les collaborations portant des libellés différents ne sont pas fusionnées. La sélection courante, l'historique, les doublons et les retraits de file sont conservés. Le petit historique récent suit la file et sa reprise chiffrée ; il est effacé à la déconnexion.

La normalisation lit `playerConfig.audioConfig.loudnessDb` sur les réponses player, y compris lorsque leurs adresses audio exigent ensuite yt-dlp. Les consultations spécifiques de niveau restent des requêtes de métadonnées annulables, sans test CDN ni téléchargement audio. Elles sont bornées et mises en cache ; une bascule sur un morceau déjà ouvert ne relance pas le flux. Le gain est `10^(-max(loudnessDb,0)/20)` : atténuation uniquement, sans compression dynamique, amplification des morceaux faibles ou réencodage. Une mesure absente / invalide donne un gain de 1. Le volume utilisateur reste indépendant du gain, y compris lors des reprises et déplacements ; aucune consultation supplémentaire de niveau courant n'est lancée en pause.

Tests ciblés : espacement des artistes, titres récents, permutation complète et historique préservé ; gain de 6 dB ≈ 0,501, absence d'amplification, valeurs inconnues / non finies et désactivation ; compatibilité et aller-retour des nouvelles préférences. L'essai explicite `normalization_uses_live_metadata_and_toggles_without_reloading` a réussi sur un vrai morceau : mesure YouTube **−2,47 dB**, donc gain **1** attendu ; les bascules conservent le volume, la pause et la position. Les réglages ne modifient pas le codec ni le débit du flux.

Validation finale : 43 tests automatiques réussis, 17 essais d'environnement ignorés par défaut. Les nouvelles préférences restent désactivées par défaut et n'écrasent pas les réglages existants de qualité / zone de notification.

## Paramètres simplifiés — 12 novembre 2026

Légende S / H / M et ligne Flux actuel retirées du dialogue ; l'option de badge du lecteur reste disponible. Section renommée Options, avec « Masquer dans la zone de notification » en première position et le même alignement que les autres interrupteurs. Pied du dialogue : « tonal v0.3 ». Le conteneur de défilement a été supprimé. Capture native `target/ui-capture/settings-v03-compact.png` inspectée à 840 × 640 : tous les contrôles et le pied sont visibles sans ascenseur. Clippy toutes cibles / toutes options réussi ; aucun changement de lecture ou de préférences enregistrées.

## Suggestions à la demande — 12 novembre 2026

Bouton Suggestions dans la bibliothèque ; vue native noire, retour à la bibliothèque, titre de référence et actualisation explicite. Jusqu'à douze propositions autour du morceau courant ou restauré. Une consultation authentifiée `next` / `RDAMVM` demande uniquement des métadonnées ; le protocole a été vérifié dans [WatchMixin de ytmusicapi](https://github.com/sigma67/ytmusicapi/blob/main/ytmusicapi/mixins/watch.py), sans reprise de code. Les parseurs de Tonal conservent les enregistrements principaux, écartent la référence, les doublons d'identifiant, les entrées indisponibles et les contreparties vidéo. Aucune recherche globale ni insertion automatique dans une playlist. Le clic présente immédiatement le titre puis utilise le streaming natif et une file complète indépendante. L'arrivée d'une page de playlist ne doit pas étendre une file sans identifiant de playlist.

Chargement sur un worker distinct de la bibliothèque et de l'audio, réponse limitée à 4 Mio, délais HTTP existants, aucune nouvelle requête automatique après erreur. Résultats stables jusqu'à actualisation ou réouverture autour d'un autre morceau ; aucune requête au simple démarrage. Déconnexion / destruction : résultats tardifs ignorés et métadonnées effacées. Sans référence, état vide explicite. La recherche reste réservée à la bibliothèque. Les propositions peuvent inclure des versions live et clips, selon les résultats de YouTube Music.

Vérifications : 45 tests automatiques réussis ; tests du parseur et des résultats tardifs / erreurs compris. Clippy toutes cibles et toutes options réussi. L'essai réel `suggestions_play_in_an_independent_queue_without_changing_the_open_playlist` a réussi avec la session existante : 12 propositions, sélection immédiate, lecture native muette puis pause, file indépendante et playlist affichée inchangée. L'essai initial dans le bac à sable n'avait pas pu valider la session ; le même essai avec l'accès réseau requis a réussi en 13,45 s. Capture native `target/ui-capture/suggestions-compact.png` inspectée à 840 × 640 : titres, artistes, pochettes et durées visibles, aucune préparation audio lors de l'ouverture.

## Boutons Suggestions et file — 12 novembre 2026

Suggestions remplace la file à gauche de Précédent ; la file est déplacée à droite de Suivant. Le bouton Suggestions de la bibliothèque est retiré. Le bouton du lecteur bascule la vue, ferme la file lors de l'ouverture et conserve la playlist consultée pour le retour. Les commandes centrales gardent leur position. Sous 1040 points de largeur, la durée monte au-dessus du slider lorsqu'il apparaît, afin de laisser la file libre. Capture native `target/ui-capture/player-buttons-compact.png` inspectée à 840 × 640 avec volume déployé et source silencieuse : aucun chevauchement des commandes, de la durée ou du volume. Clippy toutes cibles / toutes options réussi.

## Ajouter une suggestion à une playlist — 12 novembre 2026

Bouton liste / plus à droite de la file pour un morceau d'une file de suggestions ; menu natif sombre avec titre épinglé et destinations modifiables du compte. Le menu se ferme à l'extérieur ou si le titre change. Présence vérifiée avant de masquer le bouton ; erreurs / états inconnus restent explicites. Chargement indépendant de l'audio et de la pagination visible. L'endpoint `playlist/get_add_to_playlist` observé ne fournit pas `containsSelectedVideos` avec ce compte : repli sur toutes les pages de métadonnées, sans audio ni téléchargement des pochettes. Cache en mémoire de 24 catalogues et 50 000 identifiants maximum, conservé 15 minutes, effacé à la déconnexion ; une seule vérification de catalogue à la fois.

Le choix d'une destination déclenche une opération épinglée au titre et à la playlist choisis : relecture des destinations / de la présence, puis un seul `ACTION_ADD_VIDEO` via `browse/edit_playlist` si nécessaire. Aucun forçage des doublons, aucune répétition automatique d'écriture après erreur. Succès uniquement avec `STATUS_SUCCEEDED` et résultat d'ajout correspondant au bon identifiant vidéo / entrée. Le résultat met à jour la présence et les compteurs connus ; une autre lecture ne reçoit pas l'état du morceau précédent. Une déconnexion empêche les écritures non encore envoyées ; une requête déjà envoyée peut aboutir côté YouTube.

Protocole vérifié dans les sources primaires [YouTube.js — AddToPlaylistServiceEndpoint](https://github.com/LuanRT/YouTube.js/blob/main/src/parser/classes/endpoints/AddToPlaylistServiceEndpoint.ts) et [ytmusicapi — PlaylistsMixin](https://github.com/sigma67/ytmusicapi/blob/main/ytmusicapi/mixins/playlists.py), sans copie de code. Aucun cookie ni réponse authentifiée brute n'est journalisé.

Validation : **49 tests automatiques réussis**, 21 essais d'environnement ignorés par défaut ; Clippy toutes cibles / toutes options réussi. Nouveaux contrôles : destinations éditables / statut inconnu, corps d'ajout et succès explicite, présence sur une page ultérieure, erreurs / cycles / annulation, résultat tardif lié au titre d'origine, nettoyage à la déconnexion. Essai réel `playlist_membership_live_read_only` réussi en 5,03 s : une destination modifiable et présence connue confirmée sur toutes les pages, sans écriture. Capture native `target/ui-capture/playlist-add-compact.png` inspectée à 840 × 640 avec volume déployé : vrai clic simulé sur le bouton, destination réelle visible, absence d'ajout et de préparation audio confirmée. Le parcours d'écriture est couvert par les contrôles de requête / réponse ; **aucun ajout réel au compte n'a été effectué pendant les essais**. L'écriture réelle reste à confirmer après le choix d'une destination par l'utilisateur.

### Menu simplifié avec pochettes

Titre du morceau et en-tête « Ajouter à une playlist » retirés. Chaque destination présente sa pochette de 40 points et son nom, dans une ligne cliquable de 56 points avec survol et texte tronqué sans infobulle. Les pochettes viennent de la réponse du menu ; repli sur la bibliothèque par identifiant, puis sur le visuel neutre du cache existant. Le chargement reste borné et limité aux lignes visibles, sans cookies vers les hôtes d'images. Capture `target/ui-capture/playlist-add-covers-compact.png` inspectée à 840 × 640 : vraie pochette affichée et clic d'ouverture du menu validé, sans ajout ni préparation audio. Clippy toutes cibles / toutes options réussi.

Ancrage ajusté : coin inférieur gauche du menu au-dessus du bord gauche du bouton (`TOP_START`), ouverture vers la droite. Largeur adaptée à l'espace disponible à droite afin de conserver ce sens d'ouverture dans la fenêtre compacte ; aucun basculement automatique vers la gauche. Capture `target/ui-capture/playlist-add-anchor-left.png` inspectée à 840 × 640 : ouverture à droite du bouton, destination et pochette visibles, aucun ajout envoyé. Clippy toutes cibles / toutes options réussi.

Ajustement final demandé : menu centré au-dessus du bouton (`RectAlign::TOP`, centre inférieur du menu aligné sur le centre supérieur du bouton), largeur de 320 points. Il s'étend désormais des deux côtés du bouton. Clippy toutes cibles / toutes options réussi ; les captures d'ancrage précédentes décrivent les étapes antérieures.

## Ajouts optimistes, notification et actualisation — 12 novembre 2026

La sélection d'une playlist ferme immédiatement le menu, masque le bouton pour ce titre, incrémente le total connu et met à jour les listes / catalogues complets déjà présents. Le morceau disparaît aussitôt des suggestions. Notification indépendante du lecteur, ancrée en haut à droite sous la barre de fenêtre : fond blanc, texte noir et validation arrondie, « titre ajouté à playlist », visible cinq secondes. L'audio, la file courante et la position continuent normalement.

Jusqu'à huit opérations distinctes sont traitées en fond, chacune épinglée à son titre et sa playlist. Le contrôle de doublon et l'écriture restent côté worker. Une réponse d'écriture perdue est réconciliée par une lecture, sans nouvelle mutation automatique. Un échec restaure le compteur, les insertions locales et la suggestion d'origine si sa vue est encore pertinente, puis remplace la notification par une erreur avec une croix. Les résultats tardifs ne sont pas attribués au nouveau titre. Déconnexion / destruction empêchent les opérations non encore envoyées.

Après le résultat d'un ajout, la playlist complète et ses métadonnées sont relues sur un worker distinct ; deux relectures simultanées maximum, coalescées par playlist. Une révision par playlist écarte les réponses antérieures à un nouvel ajout. Les autres navigations et la pagination restent indépendantes ; la playlist sélectionnée, les totaux et le catalogue de lecture sont réconciliés avec le serveur. En cas de perte réseau pendant cette relecture, l'état local reste disponible, sans boucle de requêtes. Aucun total de page partielle n'est pris pour une playlist complète.

Chaque chargement de suggestions récupère jusqu'à 48 candidats, retire les identifiants déjà présents dans les playlists modifiables du compte, puis conserve douze propositions maximum. Les métadonnées de présence sont mises en cache, sans audio ni pochettes supplémentaires. Le filtrage s'effectue aussi après redémarrage. Une exclusion locale protège les ajouts en cours contre une réponse de suggestions déjà en vol ; une actualisation conserve cette exclusion, un échec peut la retirer. Les comparaisons portent sur l'identifiant vidéo : des enregistrements différents d'une même chanson restent des candidats distincts.

Validation : **51 tests automatiques réussis**, dont retour arrière optimiste et absence de réapparition dans les réponses tardives / rechargements. Clippy toutes cibles et toutes options réussi. Essai réel `playlist_refresh_reads_authoritative_metadata_without_writing` réussi : **620 titres** et total serveur relus en **8,06 s**, sans écriture. Captures natives `target/ui-capture/playlist-add-optimistic.png` et `playlist-add-rollback.png` inspectées à 840 × 640 : notification blanche / noire avec validation ou croix, fermeture du menu, compteur immédiat puis restauré et retrait / retour de la suggestion vérifiés. Les captures utilisent une écriture lente ou un échec simulés dans le mode de validation ; aucune mutation du compte et aucune préparation audio n'ont été effectuées pendant ces essais. Le parcours d'écriture réel reste déclenché uniquement par le choix de l'utilisateur dans l'application normale.

## Ascenseurs et commandes du lecteur — 12 novembre 2026

Ascenseur principal décalé de 20 points vers la droite, dans une marge dédiée ; le contenu conserve son centrage et sa largeur. Volume, répétition et aléatoire : dessin réduit de 28 à 21 points, comme la file, avec les zones de clic de 40 points conservées. Dans À suivre, la position de l'ascenseur reste inchangée ; son fond est transparent au repos, au survol et pendant la manipulation. Poignée et menu des lignes déplacés de 12 points vers la gauche, avec le texte ajusté.

Captures `target/ui-capture/main-scrollbar-right.png` et `queue-scrollbar-clear.png` inspectées à 840 × 640, avec survol des ascenseurs. Le contrôle du glisser-déposer a révélé un appui initial non capturé ; la poignée visible accepte désormais explicitement cet appui, en vérifiant la zone affichée et la couche du dialogue. Le scénario natif `queue-move` a ensuite réussi (`queue-move-inset.png`) : ordre suivant modifié, titre courant conservé. Source locale silencieuse en pause, aucune lecture réseau ou écriture de playlist.

## Texte noir sur fond clair et notification agrandie — 12 novembre 2026

Cause identifiée dans les sources egui / epaint 0.37 : le thème sombre sélectionne `FontColorTransferFunction::TwoCoverageMinusCoverageSq`, prévu pour le texte clair sur fond sombre. L'atlas est partagé entre couleurs, donc les textes noirs des notifications et badges utilisaient aussi cette courbe, qui renforce excessivement la couverture des pixels sur fond clair. La [documentation du moteur](https://github.com/emilk/egui/blob/0.37.0/crates/epaint/src/image.rs#L327) prévoit au contraire `LIGHT_MODE_DEFAULT` / `Off` pour le noir sur blanc. Modifier seulement la couleur du Label ou sa police ne corrige pas ce choix d'atlas.

`src/light_text.rs` utilise les mêmes définitions de police et le même moteur de rasterisation, avec une couverture dédiée au texte noir. Le thème sombre conserve son atlas d'origine. Atlas GPU distinct créé à la demande, 1024 × 1024 maximum (4 Mio), mises à jour partielles et coordonnées stables pendant les allocations de glyphes dans une même passe. Placement sur les pixels physiques et recalcul selon la densité de l'écran. Les badges gardent leur taille de 14 points et leur lettre Segoe UI Bold de 12 points, avec cette correction du lissage.

Notification : Segoe UI Semibold de **16 points**, validation cerclée de **28 points**, marges adaptées, noir sur blanc. L'icône SVG conserve des terminaisons arrondies et un rendu selon la densité physique. Contrôle automatique du mode clair, de la couverture intermédiaire des pixels et de la stabilité des coordonnées après allocation de texte supplémentaire réussi ; Clippy toutes cibles / toutes options réussi. Captures natives `notification-light-text-100.png` et `notification-light-text-150.png` inspectées dans `target/ui-capture/` : texte, accents, cercle et badge M lisibles, aucune corruption d'atlas. Les captures utilisent une source locale silencieuse en pause et un ajout simulé ; aucun ajout au compte ni lecture réseau audio.

## Notification réduite, fondus et durée en fenêtre compacte — 12 novembre 2026

Notification réduite d'environ 12,5 % en conservant ses proportions : texte de 14 points, validation cerclée de 24,5 points, largeur et marges adaptées. Le rendu dédié aux fonds clairs est conservé. Capture `target/ui-capture/notification-compact-proportions.png` inspectée, avec un ajout simulé.

Le contenu principal se fond dans le noir sur 96 points au-dessus du lecteur et sous la barre de recherche. Le haut n'est masqué que lorsque des éléments sortent de la vue ; au début / à la fin de la liste, le fondu correspondant s'efface progressivement sur les 24 derniers points de défilement pour laisser les premiers / derniers titres entièrement lisibles. Courbe de transition douce, sans recouvrir la progression, les commandes du lecteur ou la marge de l'ascenseur. Capture `content-fades-top-bottom.png` inspectée au milieu d'une playlist, à 840 × 640.

Durée compacte : retrait du maintien du slider par son simple focus clavier après un clic, et retrait du soulèvement permanent lié à la présence du bouton d'ajout. Le survol et le glissement pilotent seuls l'animation ; une sortie de souris libère le focus du slider. Les commandes auxiliaires réservent la place nécessaire au texte au repos, avec ajustement pour les durées longues, sans déplacer les commandes principales. Clippy toutes cibles / toutes options réussi. Scénario natif `volume-leave` réussi : survol, clic avec focus effectivement reçu, relâchement puis sortie de souris ; slider masqué et durée revenue à sa ligne avec le bouton d'ajout toujours présent. Capture `target/ui-capture/compact-volume-reset.png` inspectée ; source locale silencieuse en pause et aucune écriture.

## Délai de lancement et reprise des liens en cache — 12 novembre 2026

Mesure préalable réelle sur le premier titre d'une playlist : yt-dlp multi-client, **7,46 s** pour la résolution / **8,48 s** avec les deux premiers blocs ; Music seule, **6,36 s** / **6,44 s**. La résolution du lien domine cet essai. Ces mesures sont des préparations avec téléchargement initial de 512 Kio, sans sortie sonore ; elles ne mesurent pas l'instant du premier échantillon audible.

`resolution_cache.rs` conserve jusqu'à 32 descriptions de flux, uniquement en RAM, pendant 20 minutes maximum et avec deux minutes de marge avant l'expiration déclarée. Clé : vidéo canonique et empreinte de la session, avec séparation de l'anonyme. Adresses signées dans `Zeroizing<String>`, données bornées, aucun cookie ou audio dans le cache et aucun enregistrement sur disque. Une annulation est vérifiée avant et après la résolution, y compris lors d'une reprise. Une génération de cache empêche une tâche ancienne de publier un résultat après déconnexion. Les liens sans expiration explicite, presque expirés ou hors des hôtes HTTPS autorisés ne sont pas conservés.

La lecture bénéficie du cache, mais `RemoteData::renew` le contourne et invalide la clé pour une expiration ou un refus HTTP. Cela évite de rendre à nouveau le même lien refusé. La qualité et la taille du flux restent soumises aux vérifications existantes. Si la normalisation n'a pas encore de mesure et que la résolution native a échoué / été temporairement désactivée, ses métadonnées sont demandées pendant la résolution yt-dlp, en conservant l'attente de cette mesure avant le début de lecture ; aucune compression, amplification supplémentaire ou réencodage.

Validation : **55 tests automatiques réussis**, 23 essais d'environnement ignorés par défaut ; Clippy toutes cibles / toutes options réussi. Nouveaux contrôles : isolation entre comptes / anonyme, format Premium conservé, expiration / âge maximal, capacité, génération de déconnexion, rejet des hôtes non autorisés / expirations inconnues, annulation avant cache. Essai réel `cached_preparation_preserves_premium_and_measures_replay_latency` : **6,655 s à froid**, **68,51 ms en reprise**, même **Opus 774 à 261 kbit/s**, normalisation activée, deux premiers blocs compris. Essai réel `the_same_stream_can_renew_after_multiple_expirations` réussi après deux expirations simulées d'un flux réel, malgré le cache. Aucun fichier audio ni lecture sur la sortie Windows, aucune modification de playlist pendant ces essais. Le bénéfice mesuré est celui d'un titre déjà préparé ; le coût du premier lancement d'un nouveau titre reste à réduire.

Comparaison de mécanismes vérifiée dans les sources publiques : [Tubefast player.rs](https://github.com/yigitbozyaka/tubefast/blob/master/src/player.rs) conserve des liens et précharge ; [ytm-rs backend.rs](https://github.com/grapedjuice/ytm-rs/blob/main/src/backend.rs) précharge aussi et peut utiliser la qualité standard pendant la préparation du moteur Premium ; [jsc.rs](https://github.com/grapedjuice/ytm-rs/blob/main/src/jsc.rs) résout les signatures dans QuickJS embarqué avec cache du moteur. Aucun de ces projets n'a été exécuté dans un comparatif de latence ici, donc aucun temps de lancement relatif n'est affirmé. Aucun code de ces projets n'a été copié.

## Résolution embarquée des nouveaux titres — 12 novembre 2026

QuickJS via rquickjs 0.14.0 et les deux assets officiels EJS 0.8.0 remplacent l'appel systématique à yt-dlp pour les signatures et `n`. Le corps de la requête Music inclut le `signatureTimestamp` du lecteur public. Le choix garde Opus 774 puis AAC 141 lorsque ces formats sont proposés ; sinon Opus 251 puis AAC 140. Un échec ne choisit pas silencieusement un format standard si le serveur proposait un format Premium : le chemin yt-dlp existant prend le relais.

Worker dédié, file bornée, interruption par annulation / délai, limites de mémoire et de pile QuickJS ; aucun binding réseau, fichier ou processus dans JavaScript. Les téléchargements publics n'utilisent jamais les en-têtes du compte. Le seul cache disque contient une préparation publique du lecteur, sa version, son timestamp et la version EJS, dans `%LOCALAPPDATA%\Tonal\player-ejs.json`. Source et cache bornés, sauvegarde atomique et éviction d'une source invalide ; aucun bytecode non vérifié chargé depuis le disque. Préparation anticipée à la connexion ; vérification de version après une heure, runtime libéré après 90 secondes sans travail. Les caches de liens et audio, la pause réseau, le préchargement et la normalisation restent actifs.

Mesure optimisée `embedded_new_tracks_preserve_premium_and_measure_latency` sur trois titres distincts de la bibliothèque, via un appel direct qui exclut tout secours yt-dlp et tout cache de lien : résolution **218 / 184 / 167 ms** ; premiers échantillons PCM réellement disponibles **315 / 1 010 / 244 ms**. Opus Premium conservé, respectivement **261 / 254 / 257 kbit/s**. Les données sont décodées et consommées uniquement en mémoire : aucune sortie sonore, aucune écriture de playlist. Ce délai inclut la requête Music, les signatures, la vérification CDN, le premier bloc et le décodage ; il exclut la latence du périphérique Windows et le rafraîchissement egui. Il ne constitue pas une garantie réseau.

Coût de préparation séparé : première version publique non cachée, environ **7,79 s**, dont **6,60 s** jusqu'au chargement du moteur ; relance avec source déjà préparée, **348 ms**. Le coût initial est masqué par le travail en arrière-plan lorsqu'il se termine avant le premier clic. Un essai préalable avec 512 Kio préchargés a varié de 0,34 à 4,64 secondes selon le titre / serveur : obtenir le lien plus vite ne supprime pas les variations de débit CDN. Le chemin de clic manuel commence le décodage sans attendre ce préchargement du titre suivant.

Validation finale : **59 tests automatiques réussis**, 24 essais d'environnement ignorés par défaut ; Clippy toutes cibles / toutes options sans avertissement. Contrôles ajoutés : bornage et validation des identifiants publics, interruption de JavaScript en boucle, résolution des deux challenges, conservation des paramètres CDN, rejet d'origines / signatures ambiguës, qualité standard autorisée uniquement en l'absence de formats Premium. L'essai réel de renouvellement après deux expirations simulées reste réussi (**13,02 s**, source publique, sans son).

Assets EJS vérifiés par SHA-256 contre PyPI ; provenance et licences Unlicense / ISC / MIT dans `assets/ejs/`. L'orchestration Rust est indépendante, sans reprise de code GPL de ytm-rs.

## Préparation de la v0.3 — 13 novembre 2026

Version Cargo / Cargo.lock et ressources Windows harmonisées à **0.3.0** ; pied des paramètres conservé sous la forme demandée « tonal v0.3 ». Ajout des commandes Espace, Ctrl + gauche / droite et Ctrl + haut / bas, hors saisie, menus et paramètres. Espace respecte aussi la pause pendant une préparation. Suppression des réveils de 250 ms lorsque le lecteur contient un titre en pause ; les résultats de workers et les commandes Windows continuent de réveiller l'application.

Annuler une tâche Google abandonne son résultat et ferme la fenêtre de connexion sans appeler Déconnexion ni effacer une session existante. Deux ouvertures / annulations WebView2 avec une fenêtre native ouverte réussies ; rejet de session factice réussi ; aucun processus WebView2 portant le profil `tonal-login-` restant après l'essai. Les parcours réels mot de passe / clé d'accès / 2FA sont **reportés à la demande de l'utilisateur**. Le test interactif `manual_google_identity_validation_without_replacing_saved_session` est prêt et ne sauvegarde pas la session obtenue. Aucune révocation réelle du compte n'a été provoquée.

Mesures : scénario `performance_probe`, compilé uniquement avec `ui-capture`, lancé par `scripts/measure-performance.ps1`. Lecture réelle du premier titre d'une playlist, volume zéro, aucune écriture de playlist, aucune modification de la reprise sauvegardée ; échantillonnage du processus toutes les 500 ms et trois secondes exclues au début de chaque phase. Rapports et CSV dans `target/performance/20261113-035032/`. Pause visible : **0,461 % d'un cœur CPU**, mémoire privée moyenne **139,97 Mio** ; pause masquée : **0,277 %**, **139,74 Mio**. Lecture visible : **2,125 %**, **140,78 Mio** ; lecture masquée : **1,701 %**, **140,21 Mio**. Stabilisation masquée sur 100 secondes : **0,481 %**, mémoire privée finale **117,02 Mio**, minimum **113,59 Mio** ; la mémoire du runtime JavaScript est relâchée après son délai d'inactivité. Pour obtenir un équivalent du pourcentage CPU global, diviser par le nombre de processeurs logiques indiqué dans le rapport.

La première phase visible inclut encore des travaux initiaux : moyenne **12,856 % d'un cœur** et pointes courtes. Une mesure exploratoire de l'ancienne instance masquée avait capturé une pointe à 91 %, puis 2,34 % sur un autre intervalle : elle ne démontrait pas une occupation permanente d'un cœur. L'origine exacte de ces pointes initiales n'a pas été isolée ; elles ne sont pas présentées comme un défaut définitivement corrigé. Les mesures durent quelques minutes et ne prouvent pas l'absence de fuite sur plusieurs jours. Aucun changement de qualité audio ni de cache musical sur disque.

Distribution : installateur **51,14 Mio** et ZIP **63,58 Mio** dans `dist/`, exécutable **13,68 Mio**. Les outils de secours Node **v24.11.1** et yt-dlp **2026.10.26** sont trouvés dans `bin/` par rapport à l'exécutable, indépendamment du dossier courant et du PATH. Empreintes comparées aux publications officielles ; notices de licences incluses, dont les conditions GPLv3+ du binaire autonome yt-dlp séparé. Les DLL CRT viennent du répertoire redistribuable Visual Studio et sont signées Microsoft. Bootstrapper WebView2 signé Microsoft, installation conditionnée à l'absence du runtime dans le registre. Aucun Rust, SDK ou outil C++ installé par le paquet.

Essai d'installation dans un sous-dossier isolé de `target/installer-smoke/` : **19 fichiers** vérifiés contre le manifeste, versions Node / yt-dlp exécutées, puis démarrage de Tonal avec un PATH limité à Windows System32. Fenêtre « Tonal — Music » présente et réactive ; désinstallation réussie et entrée de désinstallation supprimée. Données `%LOCALAPPDATA%/Tonal` conservées. Le dossier de compilation n'est pas livré.

Validation : **59 tests réussis**, 25 essais d'environnement ignorés, Clippy toutes cibles / toutes options sans avertissement, compilation release sans `ui-capture`. L'application et l'installateur ne sont pas signés pour publication ; aucune absence d'avertissement SmartScreen ou de blocage Smart App Control n'est promise. Référence : https://learn.microsoft.com/windows/apps/package-and-deploy/smartscreen-reputation

## Gestion des playlists — 13 novembre 2026

Bouton + dans l'en-tête de bibliothèque, adjacent au rafraîchissement. Dialogue natif sombre avec flou d'arrière-plan, titre, confidentialité privée par défaut ou publique, fermeture par croix / extérieur / Échap et validation du formulaire. Modification et suppression uniquement au clic droit sur les cartes, sans nouvelles commandes permanentes sur celles-ci. Suppression confirmée dans un dialogue distinct. Une confidentialité non répertoriée existante est préservée lorsqu'on change seulement le titre.

Écritures Innertube sur un worker, une mutation à la fois, sans répétition automatique : `playlist/create`, actions `ACTION_SET_PLAYLIST_NAME` / `ACTION_SET_PLAYLIST_PRIVACY` de `browse/edit_playlist`, puis `playlist/delete`. Droits relus avant modification / suppression ; les playlists présentes dans la bibliothèque ne sont pas supposées appartenir au compte. Vérification d'annulation entre la lecture des droits et l'envoi de l'écriture. Une opération déjà envoyée peut aboutir après fermeture du dialogue ; les résultats tardifs après déconnexion sont abandonnés.

YouTube peut brièvement omettre l'en-tête éditable juste après création : le reçu de création alimente le menu local et seules les lectures de droits sont retentées brièvement. La suppression réelle renvoie parfois une réponse sans champ `status` : on confirme alors l'absence dans la bibliothèque, avec pagination bornée, sans répéter la suppression. Les réponses ambiguës restent des erreurs explicites. Les titres sont validés avant envoi ; seuls les champs réellement modifiés sont écrits.

Après confirmation : liste et titre sélectionné mis à jour, cache des destinations d'ajout invalidé, anciennes relectures de playlist abandonnées. Une suppression retire l'entrée et ses métadonnées ; la file audio connue reste lisible, sans nouvelle pagination sur la playlist supprimée. Les ajouts en cours dans la même playlist doivent être confirmés avant sa modification. Notifications réutilisées en haut à droite.

Validation locale : **62 tests réussis**, 27 essais d'environnement ignorés ; Clippy toutes cibles / toutes options sans avertissement. Tests des titres invalides, permissions, acquittements serveur, actions de confidentialité, préservation d'une confidentialité non répertoriée et abandon des résultats après déconnexion. Lecture réelle des droits / confidentialité réussie. Cycle réel sur une seule playlist privée vide de test : création, renommage, relecture, confidentialité privée conservée, suppression et absence dans la bibliothèque vérifiées (**3,87 s**). La playlist de l'essai initial interrompu a aussi été supprimée ; aucune playlist existante n'a été modifiée et aucune playlist de test ne subsiste. L'outil ponctuel de récupération utilisé pendant ce diagnostic a été retiré des tests.

Captures natives `target/ui-capture/playlist-create-compact.png`, `playlist-edit-compact.png` et `playlist-delete-compact.png` inspectées à 840 × 640. `playlist-context-compact.png` vérifie aussi le clic secondaire sur une carte réelle, la lecture des droits et l'affichage des deux actions dans le menu. Écritures désactivées pour les captures ; pas de modification du compte par les essais visuels. Exécutable release et paquets ZIP / installateur régénérés. Les conditions de publication / signature de la v0.3 restent inchangées.

### Ajustements des dialogues et du menu

Le titre utilise un champ sur une ligne de 38 points, avec marges internes explicites. La création sélectionne désormais Publique par défaut, via le même switch que les paramètres. Les boutons Annuler ont été retirés ; croix, extérieur et Échap restent disponibles. Dialogue de suppression compact et bouton rouge. Le menu affiche immédiatement « Modifier » / « Supprimer », avec une largeur calculée sur les libellés et des icônes de la même couleur que le texte, sans changement de couleur au survol. Les icônes concernées sont rasterisées à leur taille physique exacte, sur des limites alignées aux pixels.

La lecture des droits et de la confidentialité se fait en arrière-plan. Le formulaire de modification s'ouvre immédiatement ; la réponse tardive conserve le texte déjà saisi et renseigne la confidentialité réelle avant d'autoriser l'enregistrement. Une confidentialité non répertoriée reste préservée. Les écritures serveur continuent de vérifier les droits ; aucune mutation optimiste de la bibliothèque n'a été ajoutée à ce stade.

Validation : **64 tests réussis**, 27 essais d'environnement ignorés, Clippy toutes cibles / toutes options sans avertissement. Tests supplémentaires du défaut public et de l'ouverture avant réception des métadonnées sans perte de saisie. Captures finales `playlist-create-final.png` à 100 % et `playlist-context-final-150.png` à 150 % inspectées, ainsi que le dialogue de suppression compact. Aucune écriture de playlist réalisée pendant ces ajustements.

### Contenu des nouvelles playlists et dialogue de suppression

Le parseur de titres parcourt désormais uniquement les lignes directes du contenu de playlist, au lieu de collecter tous les morceaux imbriqués dans la réponse de navigation YouTube. Les recommandations voisines et leurs jetons de pagination sont exclus, y compris lorsque le contenu de la playlist vide est absent. Les doublons réellement présents dans une playlist restent conservés. Les réponses de continuation avec un identifiant de destination opaque sont acceptées lorsqu'une seule action d'ajout est présente ; les destinations de recommandations sont ignorées dans les réponses comportant plusieurs actions.

Validation : **66 tests locaux réussis**, 28 essais d'environnement ignorés ; Clippy toutes cibles / toutes options sans avertissement. Contrôles ajoutés : playlist vide avec recommandations et contenu absent, conservation de deux entrées réelles identiques, pagination propre à la playlist et destination opaque. Vérification réseau en lecture seule réussie sur les deux playlists du compte : zéro titre dans la playlist vide, 100 titres sur la première page de la playlist remplie et 100 titres sur sa page suivante. Aucune écriture sur le compte.

Le switch est intitulé « Visibilité ». Le dialogue de suppression utilise une zone distincte des formulaires et un pied limité à 40 points, avec une marge inférieure de 12 points, afin de ne plus conserver leur hauteur. Captures `playlist-delete-tight.png` et `playlist-visibility.png` inspectées à 840 × 640.

Notification : marges internes uniformes de 4 points (`Frame::inner_margin`, `Margin::same(4)`) et hauteur minimale de ligne limitée à la hauteur de l'icône (24,5 points), au lieu de celle des contrôles généraux. Capture `notification-padding4.png` inspectée avec message simulé, sans écriture sur le compte ; Clippy toutes cibles / toutes options sans avertissement.

Ajustement ultérieur : choix utilisateur de 6 points conservé en haut, en bas et à gauche, marge droite portée à 10 points. Vérification de compilation réussie.

### Thème sombre du menu de notification Windows

Le menu Ouvrir / Quitter de `tray-icon/muda` reste natif. `MenuTheme::Dark` ne concerne que la barre de menus de muda ; le rendu du popup est activé via le mode de thème du processus Windows. Résolution dynamique de `SetPreferredAppMode` / `FlushMenuThemes` depuis System32, vérification de la version avant utilisation (Windows 10 1903 ou ultérieur pour cette signature), absence d'API gérée par repli sur le rendu natif. Aucun registre ni réglage Windows modifié. Le contraste élevé conserve le mode natif par défaut. Activation au démarrage et juste avant le clic droit, afin qu'une autre bibliothèque du même processus ne laisse pas un thème différent. Aucun worker, fenêtre supplémentaire ou animation pour ce menu.

Essai graphique isolé `native_menu_background_is_dark_live` réussi : vrai popup Windows avec propriétaire masqué, deux actions Ouvrir / Quitter, lecture de son propre pixel de fond **RGB 44 / 44 / 44**, puis fermeture automatique sans sélectionner une action. Aucun pilotage d'une autre application. **73 tests locaux réussis**, 30 essais d'environnement ignorés ; Clippy toutes cibles / toutes options sans avertissement.

## Actualisation visible et transitions de lignes

La bibliothèque reste affichée jusqu'à réception des nouvelles pages ; une erreur réseau conserve les cartes. Le cache de navigation contient uniquement des métadonnées en mémoire : 8 playlists et 8 000 titres au maximum, éviction LRU, aucune écriture disque, vidé à la connexion / déconnexion. À la réouverture, les pages connues s'affichent immédiatement pendant leur relecture, puis sont remplacées ensemble. On relit uniquement jusqu'à la longueur déjà affichée ou la fin de la playlist ; aucune ancienne fin de liste n'est concaténée à une nouvelle première page. Les doublons réels sont conservés, les paginations cycliques sont rejetées, les lectures antérieures à une mutation ne remplacent pas un état plus récent et un ajout optimiste en attente reste visible. Annulation vérifiée entre les pages lors d'une annulation ou de la fermeture.

Les suggestions précédentes et leur morceau de départ restent affichés jusqu'à réussite du renouvellement. Une erreur conserve les titres jouables et propose de réessayer. Les exclusions optimistes et leurs restaurations restent actives.

Animations natives : ouverture de l'espace et fondu des nouvelles lignes, léger déplacement lors d'un réordonnancement, fondu décalé des nouvelles suggestions. Durée de 220 ms, décalage plafonné à 72 ms, maximum 24 lignes animées près de la zone visible. Une navigation vers un autre contexte ou une modification du filtre ne rejoue pas toutes les apparitions. Les identités distinguent les occurrences d'un même titre. Les lignes hors écran ne déclenchent pas de réveil et les transitions terminées ne demandent plus de rafraîchissement ; aucune mesure comparative de CPU n'a été réalisée pour cette modification.

Validation : **73 tests locaux réussis**, 29 essais d'environnement ignorés ; Clippy toutes cibles / toutes options sans avertissement. Tests du cache LRU, remplacement de pages / doublons, erreurs conservant l'affichage, cohérence des suggestions / morceau de départ, protection des ajouts optimistes, pagination cyclique, insertion / réordonnancement et fin des animations. Essai réel de lecture des métadonnées réussi : réouverture de **200 titres** immédiatement visibles pendant vérification, remplacement sans doublons, bibliothèque conservée pendant sa relecture ; sans son ni mutation de playlist. Captures natives `playlist-motion-mid.png`, `playlist-motion-settled.png` et `suggestion-motion-mid.png` inspectées à 840 × 640 avec titres simulés, sans écriture sur le compte.

## Radios NRJ / Skyrock — 16 novembre 2026

Section Radios sous les playlists de Ta bibliothèque, sans onglet supplémentaire ; logos officiels intégrés localement. Lecture native des flux MP3 publics, indépendante de Google, avec tampons bornés en mémoire, décodage sur worker et sortie audio non bloquante. Le lecteur affiche la station et DIRECT ; pause ferme la connexion, reprise rejoint le direct. Les commandes de progression, répétition et aléatoire sont désactivées pour une radio. Les redirections sont limitées aux domaines HTTPS des deux diffuseurs ; aucun cookie Google ni fichier musical utilisé. Reconnexion automatique avec délais croissants et arrêt après cinq tentatives consécutives infructueuses. Deux textures de logos à taille fixe avec mipmaps évitent d'accumuler des textures à chaque redimensionnement.

Les deux flux ont répondu HTTP 200 avec un type audio/mpeg et un débit annoncé de 128 kbit/s. L'adresse de redirection historique scdn.nrjaudio.fm a ensuite renvoyé HTTP 503 : NRJ privilégie le flux national SAT direct sur streaming.nrjaudio.fm, avec la redirection historique comme secours ; Skyrock utilise icecast.skyrock.net/s/natio_mp3_128k. Logos vérifiés depuis les ressources officielles, Skyrock en SVG ; les marges transparentes du PNG NRJ sont retirées au chargement sans modifier les proportions.

Le premier build a été arrêté par Windows Code Integrity sur un nouvel outil crc32fast (erreur 4551). Après intervention de l'utilisateur, la compilation a repris. Validation : **77 tests locaux réussis**, 32 essais d'environnement ignorés ; Clippy toutes cibles / toutes options sans avertissement. Tests des destinations autorisées, du retrait des métadonnées ICY fragmentées, de la sortie PCM non bloquante et de l'annulation d'une radio encore en préparation.

Essai réseau réel de décodage en mémoire : NRJ **44 100 Hz / stéréo**, Skyrock **48 000 Hz / stéréo**, vrais échantillons PCM obtenus sans sortie sonore ; arrêt libérant le décodeur et le lecteur réseau vérifié. Essai distinct sur sortie Windows muette : lecture, pause, reprise au direct, changement de station et arrêt réussis **sans session Google**. Aucun fichier musical écrit ni playlist modifiée.

Captures natives `radios-library.png` et `radio-player.png` inspectées à 840 × 640 : deux cartes dans la section Radios, logos officiels, nom de station dans le lecteur, DIRECT et commandes incompatibles désactivées. Les captures simulent une radio en pause sans déclencher de son. Les builds de vérification utilisent des symboles de debug désactivés et aucun cache incrémental pour limiter l'espace régénéré après nettoyage.

### Carte Radios et lecteur en direct — ajustement du 16 novembre

La section séparée est remplacée par une seule carte Radios placée après les playlists dans la même grille Bibliothèque. Cette carte de navigation reste locale : elle n'est pas ajoutée aux playlists Google ni proposée comme destination d'ajout de morceau. Elle ouvre une vue de playlist avec couverture, titre, compteur de deux radios, lignes NRJ / Skyrock et retour Bibliothèque. Aucun appel de métadonnées de playlist Google n'est lancé par cette carte. L'en-tête est renommé Bibliothèque et son sous-titre de compteur / service est retiré.

Pendant une radio, précédent est explicitement inactif dans l'interface, comme suivant, file, suggestions, répétition et aléatoire. Le volume reste utilisable. La barre rouge occupe toujours 100 %, sans poignée ni interaction de déplacement. DIRECT est remplacé par une pastille rouge ; elle reste statique en pause et ne demande pas de rafraîchissement quand la fenêtre est minimisée.

Clippy toutes cibles / toutes options sans avertissement. Captures natives radios-collection.png, radio-navigation-collection.png et radio-live-collection.png inspectées à 840 × 640. La deuxième utilise un clic réel injecté par l'outil QA de Tonal sur la carte Radios, puis vérifie l'ouverture locale et l'absence de playlist Google sélectionnée. La troisième joue réellement NRJ sur une sortie muette, afin de vérifier les commandes grisées, la barre pleine et la pastille ; aucune écriture de playlist ni de fichier musical.

Couverture simplifiée ensuite en dessin vectoriel original `assets/radios-cover.svg` : graduations de fréquences et repère rouge sur fond sombre, utilisé dans la carte et l'en-tête. Le compteur est calculé depuis STATIONS et libellé « 2 stations ». La pulsation du direct est renforcée : rayon de la pastille variant de 3,5 à 4,7 points et anneau rouge de 5 à 12 points, période de 1,6 seconde, rafraîchissement demandé toutes les 66 ms pendant la lecture visible uniquement. Captures `radios-frequencies.png` et `radios-pulse.png` inspectées, la seconde sur NRJ réellement lu en sourdine. Compilation ui-capture et formatage vérifiés.

### Collection de radios personnalisable — 17 novembre 2026

Le bouton + de la vue Radios reprend le même composant natif que l'ajout de playlist. Un dialogue avec flou, croix et fermeture extérieure propose une recherche par nom via Radio Browser. Aucune recherche au lancement ni à la seule ouverture du dialogue : deux caractères minimum et temporisation de 450 ms. La requête conserve au plus 40 résultats compatibles après filtrage, à partir de 60 entrées demandées ; réponse bornée à 512 Kio et délai global de 20 secondes. Recherche précédente annulée à la nouvelle saisie ou à la fermeture, réponses tardives isolées. Découverte des miroirs courants, puis essais bornés ; pas de catalogue complet chargé.

NRJ et Skyrock restent les stations initiales. Ajout sans doublons par identifiant / URL, compteur recalculé, coche dans les résultats ; retrait au clic droit. La collection est écrite atomiquement dans %LOCALAPPDATA%/Tonal/radios.json, indépendante de Google, limitée à 200 stations et 512 Kio. Une collection vide reste vide au redémarrage ; un échec d'enregistrement ne modifie pas la liste en mémoire. Aucune écriture Google. Les logos de l'annuaire sont chargés seulement pour les résultats visibles, par le cache borné existant ; initiales en repli si le visuel manque ou échoue.

Lecture des stations ajoutées en MP3 direct hors HLS, avec les mêmes files compressées / PCM bornées, pause coupant le réseau et reprise au direct. HTTP et HTTPS publics acceptés, ports de radio autorisés, sans en-têtes de compte ; identifiants URL et cibles locales / privées refusés. DNS résolu et vérifié avant connexion, résultats épinglés et redirections revalidées. Les deux radios intégrées gardent leur liste de domaines stricte. Aucun fichier musical enregistré.

Contrôles : 80 tests locaux réussis, 33 essais d'environnement ignorés par défaut ; Clippy toutes cibles / toutes options sans avertissement. Essai réseau explicite : recherche FIP et vrais échantillons PCM décodés depuis une station renvoyée par l'annuaire, sans sortie sonore. Essai distinct : NRJ / Skyrock jouées sur sortie Windows muette, pause, reprise, changement et arrêt réussis sans compte Google. Tests locaux de filtrage, doublons, persistance, collection vide et rejet d'URL locales. Captures natives radio-library-folder.png, radio-library-open.png et radio-library-added.png inspectées ; clic injecté sur le vrai bouton +, puis véritable recherche FIP et clic d'ajout. Le mode QA n'enregistre pas la station dans le profil utilisateur.

Documentation API consultée : https://api.radio-browser.info/ et https://de1.api.radio-browser.info/ (recherche, UUID, url_resolved, codec et HLS).

Ajustement visuel du dialogue radio : mention Radio Browser retirée de l'interface, trois opacités de fond de l'ascenseur à zéro comme dans À suivre et boutons d'ajout reculés de 14 points vers la gauche. Hauteur des résultats réservée explicitement pour conserver plusieurs lignes visibles sans le pied de dialogue. Capture radio-scroll-clean.png inspectée ; recherche FIP et clic d'ajout QA réussis, sans écriture utilisateur.

### Suivi de la sortie Windows par défaut — 17 novembre 2026

Inscription native IMMNotificationClient sur un thread COM MTA, libérée à la fermeture. OnDefaultDeviceChanged ne réveille le routeur que pour eRender / eConsole, le rôle effectivement utilisé par CPAL WASAPI ; changement de microphone ou de sortie de communication ignoré. Aucune énumération périodique, aucun choix de périphérique dans l'interface et aucune modification du réglage Windows. Les événements sont regroupés sur 180 ms ; une ouverture temporairement impossible est retentée avec des délais bornés, cinq essais au maximum par changement.

La sortie physique peut être remplacée sans reconstruire le Sink ni le décodeur : un routeur conserve le même mixeur et retire l'ancien consommateur. Le callback prélève des blocs de 256 trames avec verrou non bloquant ; silence temporaire si le mixeur est occupé, aucune attente réseau. Le nouveau flux adapte les canaux / fréquence au périphérique via rodio. Titre, volume, normalisation, position et pause restent conservés, sans re-résolution YouTube ni redémarrage du flux radio lors d'un transfert sain. Une brève interruption du son reste possible pendant le changement matériel. Si la perte de sortie avait déjà interrompu la lecture, le changement de périphérique rétablit l'intention de lecture ; une Pause explicite annule cette reprise.

Contrôles : Clippy toutes cibles / options sans avertissement ; 84 tests locaux réussis. Tests déterministes du retrait de l'ancien lecteur, de la conservation de la file, de l'adaptation stéréo 48 kHz vers mono 44,1 kHz, du callback non bloquant et du filtrage des rôles Core Audio. Essai réel sur sortie Windows muette : remplacement du flux pendant la lecture puis en pause, métadonnées / volume / position / normalisation conservés et reprise réussie. Inscription et désinscription Core Audio réelles vérifiées, avec libération des références. Lecture NRJ / Skyrock muette, pause / reprise / changement / arrêt réussis après modification du chemin audio. Essai intégré distinct : notifications de changement injectées dans AppState, transfert d'une radio sans remplacer sa connexion, reprise après perte simulée de sortie et respect d'une pause explicite après perte.

Ces essais ouvrent réellement les sorties Windows mais ne changent pas le périphérique global et ne simulent pas un branchement Bluetooth physique. L'essai avec les écouteurs de l'utilisateur reste à confirmer.

Références : https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nf-mmdeviceapi-immnotificationclient-ondefaultdevicechanged et https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nn-mmdeviceapi-immnotificationclient ; rôle confirmé dans la source CPAL 0.16.0 WASAPI.

### Morceau en cours sur les radios — 17 novembre 2026

Les flux radio demandent désormais les métadonnées ICY sur leur connexion audio existante. Les blocs StreamTitle sont retirés avant décodage et lus avec une limite de 4080 octets ; titres UTF-8 et encodages latins courants pris en charge. L'absence de mise à jour conserve le dernier titre ; StreamTitle vide ou identifiants numériques internes le réinitialisent. Les suffixes techniques § suivis d'un identifiant long sont retirés. Aucun service de reconnaissance musicale ni interrogation périodique supplémentaire.

Le nom de la station reste en première ligne du lecteur. La seconde affiche artiste — titre uniquement lorsqu'un couple exploitable est fourni. Filtre conservateur des noms d'émission reconnus : nom de station utilisé comme artiste, libellés d'émission et informations incomplètes restent sur Radio en direct. Ce n'est pas une reconnaissance du morceau : le contenu et la fiabilité dépendent de la station. Le même texte est publié dans les commandes multimédias Windows. Dernier morceau conservé pendant la pause ; informations effacées au changement de station et au retour au direct, pour éviter un ancien titre.

87 tests locaux réussis, 36 essais d'environnement ignorés par défaut ; Clippy toutes cibles / options réussi. Tests des apostrophes, accents latins, effacement explicite, identifiants, émissions et blocs fragmentés ; les données envoyées au décodeur restent strictement audio. Décodage réel muet NRJ 44100 Hz stéréo et Skyrock 48000 Hz stéréo, libération des workers vérifiée. Skyrock a réellement transmis Tame Impala - Loser avec un identifiant technique, couvert par un test de nettoyage. Une lecture préliminaire avait renvoyé SKYROCK - Le 16/20, rejetée comme émission ; NRJ avait renvoyé des identifiants numériques, masqués. Aucun titre inventé si la source ne fournit pas de morceau exploitable.
