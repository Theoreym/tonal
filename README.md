# Tonal — v0.3 (Windows)

Installation : [guide rapide](docs/QUICKSTART.md). Les paquets Windows sont générés dans `dist/` avec `scripts/package.ps1` : installateur par utilisateur et ZIP autonome, sans Rust à installer. Node.js et yt-dlp sont inclus pour le secours, avec empreintes officielles vérifiées ; les DLL C++ sont livrées à côté de l'application. WebView2 reste limité à la connexion Google. Tonal n'est pas encore signé pour la publication : un avertissement ou blocage Windows reste possible.

Tonal est un lecteur musical **natif Rust / egui 0.37 / OpenGL**, à thème **noir absolu `#000000`**. Les cartes utilisent `#141414`. **Spotify reste désactivé ; le bloc visuel des services a été retiré.**

L'interface présente la bibliothèque au centre, avec un lecteur fixe et des boutons arrondis. Le logo officiel YouTube Music est embarqué en SVG. Le bouton en haut à droite affiche le prénom et la photo du compte connecté ; son menu donne accès aux paramètres et à la déconnexion. Le filtre de bibliothèque agit sur les playlists ou les titres affichés. La barre latérale, l'écran Fichiers locaux, le bloc de saisie d'URL et le bloc YouTube Music / Spotify ont été retirés.

La barre de fenêtre est intégrée au thème noir : **tonal. · recherche · compte · minimiser · agrandir/restaurer · fermer**. Les espaces libres permettent de déplacer la fenêtre, un double-clic agrandit/restaure, et les bords permettent le redimensionnement. La recherche filtre les métadonnées déjà chargées après connexion (`Ctrl+F`). L'icône de l'application est un **t minuscule blanc sur noir**, intégré à la fenêtre et à l'exécutable Windows.

Les textes de l'interface ne sont pas sélectionnables : survoler le nom ou l'artiste d'un titre conserve le survol et le clic de sa ligne. Le champ de recherche reste éditable, avec son texte centré verticalement sur la loupe.

Sous Windows 11, l'arrondi réel de la fenêtre est demandé au compositeur Windows via DWM. Windows gère les coins des fenêtres maximisées ou ancrées ; sur une version non compatible, la fenêtre reste utilisable. Les boutons partagent un rayon de 10 points et le menu du compte utilise des lignes alignées, des icônes à gauche et un panneau arrondi avec ombre discrète.

Le menu de la zone de notification reste natif Windows, avec Ouvrir / Quitter. Tonal active son rendu sombre pour son propre processus, y compris avant chaque ouverture du menu ; les réglages Windows restent inchangés. Le contraste élevé de Windows est respecté et les versions non compatibles conservent leur rendu natif par défaut.

Les paramètres utilisent un dialogue modal avec fond réellement flouté en OpenGL, fermé par la croix, un clic à l'extérieur ou Échap. Ils affichent la photo et le nom du compte YouTube Music, son statut Premium / Standard (ou non vérifié si absent de la réponse), ainsi que Spotify grisé avec **SOON**. Le statut vient du booléen `IS_SUBSCRIBER` de la page YouTube Music authentifiée, indépendamment du débit du morceau. Le volume et l'apparence ont été retirés des paramètres ; le thème reste fixe.

L'option « Afficher la qualité du titre », activée par défaut, ajoute un carré à côté du titre du lecteur : **S** blanc pour Standard (< 160 kbit/s, y compris la plage 150–160), **H** vert pour High (160 à moins de 250), **M** doré pour Master (250 et plus). Ces appellations sont les paliers visuels demandés pour Tonal : Master ne certifie pas un fichier studio ou lossless. Le badge suit le débit du flux résolu et reste absent tant qu'il est inconnu. Le réglage est enregistré séparément dans `%LOCALAPPDATA%\Tonal\preferences.json`. Les petites icônes sont rasterisées selon leur taille physique et la densité de pixels, avec anticrénelage par mipmaps.

Les lettres S / H / M utilisent maintenant la même police en gras de 12 points et le moteur de texte Skrifa / Vello CPU d'egui 0.37, avec hinting. Le carré reste de 14 points et son bas est aligné sur la ligne de base du titre. Les logos officiels YouTube Music et Spotify sont rasterisés à deux fois leur taille physique d'affichage, mis en cache par taille / DPI, puis réduits avec filtrage adapté ; les proportions du SVG sont conservées.

Le texte noir des badges et notifications utilise un atlas de caractères distinct, avec le lissage prévu pour un fond clair. Le texte clair de l'interface conserve son traitement pour le fond noir. La notification affiche un texte Segoe UI Semibold de 14 points et une validation cerclée de 24,5 points. Le contenu défilant se fond progressivement dans le noir sur 96 points en haut et en bas ; chaque fondu s'efface lorsqu'on atteint l'extrémité correspondante de la liste.

Le lecteur réserve 8 points de marge autour du visuel, affiche le titre en gras et l'artiste en dessous, et centre les commandes principales agrandies. La progression occupe toute la séparation supérieure du lecteur. Dès le clic, le titre, l'artiste, la durée connue et la pochette sélectionnée apparaissent dans le lecteur, même pendant la préparation audio. Un autre clic annule la préparation précédente. Les pochettes absentes du cache arrivent en arrière-plan. Le texte des boutons est centré sur son dessin réel pour s'aligner avec les icônes.

La progression est rouge. Les commandes utilisent les icônes arrondies Lucide ; lecture/pause est agrandie sans cercle. À droite : durée `0:23 / 2:54`, volume, répétition puis aléatoire. Survoler le coin inférieur droit révèle le slider à gauche du volume, avec une barre blanche sur gris et une petite poignée blanche. Les flèches du clavier ajustent le volume de 1 % (5 % avec Maj). La répétition alterne arrêt / playlist / titre. La file de lecture s'étend lorsque les pages suivantes de la playlist arrivent, en conservant le titre courant et l'historique. Un clic droit sur lecture/pause permet d'arrêter.

L'aléatoire couvre **toute la playlist** : si la liste est partielle, ses métadonnées restantes sont récupérées en arrière-plan, une page à la fois. Le morceau courant continue ; une petite animation sur le bouton indique la préparation. Le prochain tirage attend le catalogue complet, sans se limiter à la page visible. Le préchargement audio est recalculé sur cet ordre complet. Aucun téléchargement de tous les morceaux ni de toutes les pochettes n'est lancé ; l'affichage reste paginé au défilement. L'historique réel est conservé, y compris quand l'aléatoire est activé en milieu de playlist. Le catalogue en mémoire est réutilisé entre sélections dans la même vue et invalidé à la réouverture / actualisation. Une erreur permet de réessayer avec Suivant, sans boucle de requêtes.

La progression est cliquable et déplaçable : un petit point rouge apparaît au survol ; pendant le glissement, la position est prévisualisée et le déplacement audio est appliqué au relâchement. La pause est conservée. Les vraies pochettes apparaissent dans la bibliothèque, l'en-tête de playlist, les lignes de morceaux et le lecteur. Chaque ligne présente le titre, l'artiste, l'album et la durée lorsqu'ils sont disponibles. Les playlists système J'aime / épisodes enregistrés / historique / à regarder et les mix automatiques sont masqués par identifiant, sans filtrer les noms des playlists personnelles.

Les images PNG, JPEG et WebP sont téléchargées en arrière-plan, sans cookies Google : quatre requêtes simultanées au maximum, cache mémoire de 64 images et dimensions limitées. Un visuel neutre reste affiché pendant le chargement ou si aucune pochette n'est disponible.

Un échec d'image n'est plus permanent : les visuels visibles sont retentés après 2, 4, 8 secondes, jusqu'à un intervalle maximal de 2 minutes. Les textures déjà chargées restent en cache ; une coupure réseau ne les efface pas.

Les titres suivants se chargent automatiquement à l'approche du bas de la liste, une page à la fois. Aucun bouton « Plus de titres » ni compteur de titres chargés n'est affiché ; une erreur permet de réessayer sans relancer les requêtes en boucle. Le titre en cours est plus clair que le survol, avec sa pochette assombrie et Pause (ou Play si en pause) ; cliquer sa ligne bascule lecture/pause. Le survol d'une carte de playlist affiche uniquement Play dans l'angle inférieur droit, sur un dégradé sombre discret et animé, sans bordure ni cercle blanc.

Sous le nom d'une playlist, « n titres » indique son total issu des métadonnées YouTube Music, indépendamment du nombre de pages chargées. Si le total n'est pas fourni, il est affiché uniquement après chargement complet. Les titres, artistes et cartes de playlists n'affichent plus d'infobulles, y compris lorsque leur texte est tronqué ; les aides des commandes restent disponibles.

L'interface et le moteur audio ne reposent pas sur Chromium. Une fenêtre **WebView2 temporaire** peut être ouverte pour la connexion Google expérimentale ; elle est fermée avant les requêtes natives de métadonnées.

La fenêtre Google utilise une classe Windows distincte de celle de l'interface native. Un essai d'intégration ouvre et annule deux connexions pendant que l'interface egui reste active, pour couvrir leur cohabitation dans le même processus.

Les transferts `SetSID` / `CheckCookie` vers les domaines régionaux Google sont permis à partir d'une liste officielle embarquée. Une destination de transfert encore refusée produit une erreur explicite. Le journal expurgé `tonal-login.log`, placé à côté de l'exécutable et remplacé à chaque tentative, aide à diagnostiquer les étapes de connexion sans conserver d'identifiants ni de cookies.

## Fonctionnalités

- **Radios en direct** : carte Radios à la suite des playlists dans Bibliothèque. Elle ouvre une vue de playlist avec NRJ et Skyrock, leurs logos et les mêmes lignes de lecture que les morceaux. Lecture native MP3 indépendante du compte Google, tampons bornés en mémoire, aucune musique enregistrée sur disque. La pause coupe le flux et la reprise rejoint le direct. Le lecteur affiche une pastille rouge pulsante et une barre rouge pleine non déplaçable ; seul le transport lecture/pause reste actif. Reconnexion automatique bornée en cas de coupure. L'en-tête Bibliothèque n'affiche plus de compteur ni de libellé YouTube Music.

- **Actualisations sans écran vide** : bibliothèque conservée pendant sa relecture ; playlists réaffichées depuis un cache de métadonnées en mémoire (8 playlists / 8 000 titres maximum), puis vérifiées sur la plage déjà connue. Les nouvelles pages remplacent cette plage en une fois, sans mélange avec une ancienne fin de liste. Les suggestions précédentes restent lisibles pendant leur renouvellement et en cas d'erreur réseau. Cache vidé à la déconnexion. Les insertions visibles ouvrent progressivement l'espace entre les lignes ; les suggestions arrivent avec un léger fondu et déplacement. Transitions de 220 ms, décalage entre lignes plafonné à 72 ms, sans réveil d'animation au repos.

- **Gestion des playlists YouTube Music** : bouton `+` près du rafraîchissement de la bibliothèque ; dialogue natif compact avec titre sur une ligne et switch Visibilité, activé par défaut. Clic droit immédiat sur une carte : menu « Modifier » / « Supprimer », de largeur adaptée aux libellés. Les métadonnées arrivent en arrière-plan et les droits restent vérifiés côté serveur. Une confidentialité existante inconnue n'est jamais remplacée par la valeur par défaut ; le titre déjà saisi reste conservé pendant son chargement. Suppression confirmée dans un petit dialogue avec bouton rouge ; fermeture par croix, clic extérieur ou Échap. Les écritures sont traitées en fond, sans répétition automatique. Bibliothèque et destinations d'ajout sont mises à jour après confirmation, sans interrompre la lecture en cours.

- **Résolution embarquée des nouveaux titres** : Innertube fournit les formats autorisés pour le compte ; QuickJS et le solveur officiel EJS 0.8.0 traitent les signatures et le paramètre `n` sans lancer de processus externe. Le lecteur public est préparé en arrière-plan à la connexion et sa préparation est conservée sur disque par version. Ce cache ne contient ni musique, ni cookies, ni liens audio signés. yt-dlp reste disponible en secours. Les formats Premium sont prioritaires ; le standard est utilisé si aucun format Premium n'est proposé.
- **Relance des titres accélérée** : les liens audio déjà résolus sont conservés uniquement en mémoire, pour le même compte, avec 32 entrées maximum, une durée maximale de 20 minutes et au moins deux minutes de marge avant expiration. Aucun fichier musical ni lien signé n'est enregistré sur disque. Le renouvellement d'un lien expiré / refusé force une résolution fraîche ; la déconnexion vide le cache et empêche les anciennes tâches de le repeupler. La normalisation manquante est récupérée en parallèle du repli yt-dlp. Mesure antérieure au moteur embarqué : préparation et premiers 512 Kio en **6,66 s** à froid, puis **69 ms** en reprise, même Opus à **261 kbit/s**. Les nouvelles mesures sont consignées dans `docs/VALIDATION.md`.

- **Ajouter une suggestion à une playlist** : un bouton liste / plus apparaît à droite de la file lorsque le titre d'une file de suggestions n'est présent dans aucune playlist modifiable du compte. Le menu natif affiche uniquement les pochettes et les noms des playlists YouTube Music. Au choix d'une playlist, l'interface est optimiste : menu fermé, compteur et listes connues mis à jour, morceau retiré des suggestions, notification blanche à texte noir avec validation en haut à droite (« titre ajouté à playlist »). L'écriture est traitée en fond et reste liée au bon titre ; un échec rétablit le morceau et le compteur avec une notification d'erreur. Après confirmation, la playlist complète et ses métadonnées sont relues en arrière-plan ; un résultat antérieur à un nouvel ajout est écarté. Présence vérifiée sur toutes les pages, sans forcer les doublons ni répéter une écriture automatiquement. En petite fenêtre, la durée et le volume gardent la place nécessaire.

- **Suggestions à la demande** : le bouton Suggestions du lecteur, à gauche de Précédent, propose jusqu'à 12 morceaux autour du titre courant ou restauré, avec pochettes, artistes, albums et durées disponibles. Jusqu'à 48 candidats YouTube Music sont filtrés pour retirer les titres déjà présents dans les playlists modifiables du compte. Ce filtrage se refait à chaque chargement, y compris après redémarrage ; les ajouts en cours restent exclus des réponses tardives. Aucun moteur de recherche global n'est exposé. Le titre servant de référence est affiché, et Actualiser permet de repartir du morceau courant. Ouvrir cette vue ne télécharge que des métadonnées et les pochettes visibles. Cliquer une proposition crée une file Tonal indépendante, sans modifier les playlists. Sans titre de référence, un message invite à écouter un premier morceau. Les erreurs permettent une relance manuelle, sans requêtes en boucle ; la déconnexion efface les suggestions.

- Deux options dans Paramètres → Options, désactivées par défaut : **Aléatoire intelligent**, qui réorganise les suivants en mode aléatoire pour espacer les artistes et les 20 titres récents, et **Normaliser le volume**, qui applique une atténuation constante selon le `loudnessDb` fourni par YouTube. Les morceaux faibles ne sont pas amplifiés ; sans mesure, le son reste inchangé. Le volume choisi, la position et le flux Premium restent conservés, sans réencodage. Les paramètres sont mémorisés ; le courant et l'historique restent stables lors du changement de mode. Les paramètres tiennent sans ascenseur dans une fenêtre de 840 × 640 : la légende S / H / M et le flux actuel ont été retirés. « Masquer dans la zone de notification » est la première option, alignée avec les autres interrupteurs ; le pied du dialogue affiche « tonal v0.3 ».

- Moteur natif rodio : lecture, pause, reprise, arrêt, volume et remise à zéro de l'affichage en fin de titre. Le décodage MP3, FLAC, WAV et OGG/Vorbis reste couvert par les essais techniques ; l'interface n'expose plus d'ouverture de fichiers locaux.
- **Streaming natif Opus / AAC**, sans WAV temporaire ni FFmpeg pour la lecture YouTube. Rust résout les liens avec QuickJS embarqué, puis télécharge des blocs HTTP de 256 Kio dans un cache de 8 Mio maximum ; Symphonia décode l'AAC et libopus l'Opus, via des bindings Rust, sur un thread séparé de rodio. En secours, yt-dlp essaie le client Music seul avant le repli général. Le début du prochain morceau est préchargé (512 Kio maximum), sans stocker de musique sur disque. libopus remplace le décodeur expérimental, sans modifier ni réencoder le flux Premium.
- La lecture séquentielle continue sans défilement : lorsqu'il reste cinq titres connus ou moins, les métadonnées restantes de la playlist sont récupérées par pages en arrière-plan. L'affichage conserve sa pagination progressive ; les morceaux ne sont pas tous téléchargés. Une erreur permet de réessayer avec Suivant.
- Le bouton de file, à droite de Suivant, ouvre un panneau central au-dessus du lecteur : morceau courant, suivants déplaçables, « Lire maintenant » et « Retirer de la file ». La croix, un clic extérieur et Échap le ferment. Les modifications concernent uniquement la file de Tonal ; les playlists YouTube restent intactes.
- La file, son ordre, son historique courant, le shuffle, la répétition, le volume et la position sont sauvegardés dans `%LOCALAPPDATA%\Tonal\playback.dpapi`, chiffrés par DPAPI et liés à la session. Au prochain démarrage, le morceau est présenté **en pause**, sans préparer l'audio avant Lecture. Sauvegarde lors des changements de file / modes / volume, environ toutes les dix secondes pendant la lecture, et à la fermeture. Arrêt et déconnexion effacent la reprise. Aucun lien audio signé ni cookie n'est inclus dans cette sauvegarde.
- Commandes multimédias Windows : titre, artiste, pochette, durée et progression ; Lecture, Pause, Suivant, Précédent et déplacement via SMTC. La pause demandée pendant le chargement est appliquée à l'ouverture du flux. L'option « Masquer dans la zone de notification », désactivée par défaut, masque la fenêtre et conserve la lecture ; clic gauche sur l'icône pour réafficher, menu pour Afficher / Quitter.
- La pause suspend le décodage, interrompt les requêtes audio en cours et ferme leur pool de connexions. Elle annule aussi le préchargement du prochain titre ; aucune nouvelle requête audio n'est lancée jusqu'à la reprise. La position et le cache du titre courant sont conservés, y compris pour un déplacement en pause. Cela ne garantit pas un délai de libération immédiat du côté de YouTube.
- Après une pause d'au moins 30 minutes, Reprendre réouvre la sortie Windows et prépare un flux frais au même endroit dans le morceau, en conservant le volume et la file. Une erreur de sortie ou de décodage conserve aussi le titre et la position : Lecture permet de réessayer sans relancer l'application, sans boucle automatique de requêtes. Les liens audio expirant dans moins d'une minute sont renouvelés avant un bloc manquant ; un refus HTTP 403 / 410 permet aussi un renouvellement, limité à une tentative par lecture de bloc et compatible avec plusieurs expirations successives du même morceau.
- La session Google permet de demander les formats Premium **Opus 774 / AAC 141, autour de 256 kbit/s**, quand YouTube les fournit. Sinon, repli sur Opus 251 / AAC 140 standard. Le badge de qualité facultatif reste disponible à côté du titre du lecteur. Aucun suréchantillonnage ou transcodage pour fabriquer une qualité supérieure.
- Préparation de la connexion Google dans une fenêtre WebView2 avec un profil temporaire isolé. La navigation privée n'est pas imposée afin de conserver le comportement normal des clés d'accès ; les méthodes proposées par Google restent disponibles. Annulation possible ; délai maximal de connexion de dix minutes. Tonal affiche séparément l'attente de validation Google et la vérification de session.
- Backend natif YouTube Music : vérification de session, récupération des playlists de bibliothèque, pagination, ouverture d'une playlist et pagination des titres. Affichage dans egui.
- Session chiffrée par **DPAPI pour le compte Windows courant**, dans `%LOCALAPPDATA%\Tonal\google-session.dpapi`. Aucun mot de passe n'est lu par le code de Tonal. Bouton de déconnexion et effacement de la session locale.

**La connexion réelle et l'affichage de la bibliothèque ont été confirmés par l'utilisateur le 11 novembre 2026**, après correction du transfert via `accounts.google.fr`. La pagination réelle, la restauration après redémarrage et la connexion par clé d'accès restent à valider séparément. Google peut refuser les navigateurs intégrés. Il s'agit d'une session web réutilisée, pas d'un flux OAuth officiel. L'accès Innertube est non officiel et peut changer.

La résolution audio réutilise la session Google via une configuration envoyée sur l'entrée standard privée de yt-dlp : aucun cookie dans les arguments du processus, les journaux ou un fichier de cookies en clair. Une version contenant le correctif de portée des cookies (2023.07.06 ou ultérieure) est exigée. Les requêtes audio natives vers les serveurs Google Video ne transmettent aucun cookie. Les restrictions du compte et du contenu restent appliquées par YouTube.

## Lancer sous Windows

Prérequis :

1. Rust stable x64 MSVC 1.95 ou ultérieur via [rustup](https://rustup.rs/).
2. Visual Studio Build Tools 2022 ou ultérieur, avec **Desktop development with C++** et le SDK Windows.
3. Runtime Microsoft Edge WebView2 pour la connexion uniquement.
4. yt-dlp récent et Node.js dans le PATH pour le chemin de secours. Le chemin principal utilise QuickJS embarqué et ne dépend pas d'un moteur JavaScript installé. FFmpeg est utilisé uniquement par certains essais techniques.
5. CMake dans le PATH pour compiler libopus. Sur cette machine, un CMake portable est placé dans `target/build-tools/cmake-wheel/cmake/data/bin/` et détecté par `scripts/run.ps1`. Il n'est pas nécessaire pour utiliser l'exécutable déjà compilé.

Sur la machine de développement utilisée le 11 novembre 2026, Rust, les outils C++, le SDK et yt-dlp ont été installés ; FFmpeg et WebView2 étaient déjà présents.

```powershell
# Depuis le dossier du projet
.\scripts\run.ps1
```

Le script actualise le PATH du processus pour retrouver les outils récemment installés. Il fonctionne avec Windows PowerShell 5.1 et PowerShell 7. Autre possibilité, après réouverture du terminal :

```powershell
cargo run --release --locked
```

L'exécutable est **`target\release\tonal.exe`**. `Cargo.lock` est conservé pour stabiliser les dépendances.

Si nécessaire pour une autre machine :

```powershell
winget install --id Rustlang.Rustup --exact
winget install --id yt-dlp.yt-dlp --exact
winget install --id Gyan.FFmpeg --exact
```

## Essayer la connexion et les playlists

1. Cliquer sur **Se connecter à Google** dans Tonal.
2. Saisir soi-même ses identifiants sur la page Google dans la fenêtre temporaire. Si Google refuse WebView2, fermer la fenêtre ; aucun contournement du refus n'est prévu.
3. Après retour sur YouTube Music et récupération des cookies de session, Tonal ferme WebView2, puis vérifie la session et demande les playlists par HTTP natif. **Session vérifiée** n'apparaît qu'après cette vérification serveur et le premier chargement de bibliothèque.
4. Ouvrir une playlist pour afficher ses titres ; les pages suivantes arrivent automatiquement à l'approche du bas. Cliquer un titre l'affiche immédiatement dans le lecteur puis démarre le flux. **Plus de playlists** charge les playlists suivantes ; **Actualiser les playlists** recharge la bibliothèque.
5. **Déconnecter** efface les données de compte affichées et la session locale. Cela ne révoque pas la connexion dans les paramètres du compte Google.

Le premier compte Google du profil temporaire est utilisé (`authuser=0`). La sélection de comptes secondaires / chaînes de marque reste à développer. Une session expirée ou révoquée demande une reconnexion. Les erreurs réseau ne sont pas considérées comme une déconnexion.

## Vérification

```powershell
cargo check --locked
cargo fmt -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked

# Tests réels optionnels : courts sons faibles, fenêtre de connexion automatiquement
# annulée, requêtes réseau et lecture native publique / authentifiée.
cargo test --locked -- --include-ignored --test-threads=1
```

Les tests réels nécessitent FFmpeg (génération de fixtures), yt-dlp, Internet, une session Windows interactive et une sortie audio. Les deux essais authentifiés réutilisent la session DPAPI déjà enregistrée ; celui des transitions utilise les premiers titres de la première playlist, avec le volume coupé. Les essais audio réseau publics utilisent une courte vidéo de contrôle.

Le détail des validations et de leurs limites figure dans [docs/VALIDATION.md](docs/VALIDATION.md).

## Architecture

```text
src/main.rs       Interface native egui et thème
src/ui.rs         Thème noir, icônes vectorielles et boutons accessibles
src/window_chrome.rs Barre de fenêtre, commandes Windows et redimensionnement
src/backdrop.rs   Flou GPU de l'interface sous le dialogue de paramètres
src/preferences.rs Préférences d'interface persistantes, sans données de connexion
src/app_state.rs  État de l'application, tâches de fond, résultats et déconnexion
src/audio.rs      Sortie système et lecture rodio
src/native_stream.rs Blocs HTTP bornés, décodage Opus/AAC, déplacement et file PCM
src/player_solver.rs QuickJS embarqué, préparation du lecteur public et signatures / n
src/http_audio.rs Requêtes audio asynchrones annulables, interruption réseau à la pause
src/artwork.rs    Chargement natif et cache borné des pochettes et avatars
src/brands.rs     Logos SVG officiels, cache par taille physique / DPI et filtrage
src/playback_queue.rs File de playlist, historique, aléatoire et répétition
src/queue_panel.rs Panneau temporaire de file et déplacement des titres
src/playback_store.rs Reprise chiffrée DPAPI, sauvegarde atomique hors du fil UI
src/desktop.rs    SMTC Windows et zone de notification facultative
src/direct_audio.rs Résolution HTTP native des liens exploitables, sans déchiffrement JS
src/auth.rs       Fenêtre WebView2 temporaire, annulation et destruction
src/session.rs    Secrets en mémoire et stockage chiffré DPAPI
src/provider.rs   Provider de métadonnées, pagination et frontière StreamResolver
src/youtube.rs    Résolution d'URL / qualité par yt-dlp, session sur entrée privée
scripts/run.ps1   Lancement Windows et actualisation du PATH
assets/brands/    Logos officiels embarqués et leurs sources
assets/tonal.*    Icône originale : SVG, PNG et ICO Windows multi-résolution
```

Pour régénérer l'icône depuis son SVG : `cargo run --example export_icon --features ui-capture`. `build.rs` intègre automatiquement l'ICO à l'exécutable.

Les requêtes réseau, la résolution et le décodage audio sont exécutés hors du fil egui. La sortie rodio n'attend jamais une requête réseau. Les résultats tardifs d'une tâche abandonnée ne changent pas la sélection courante et ne rétablissent pas une session déconnectée. Les workers réseau n'enregistrent jamais eux-mêmes de session.

## Limites de l'alpha

- La recherche reste limitée à la bibliothèque de l'utilisateur. Les suggestions liées au titre courant sont disponibles ; les favoris synchronisés restent prévus et le mini-lecteur est à réévaluer selon l'usage. La normalisation facultative et l'aléatoire intelligent sont disponibles dans les paramètres.
- La préparation initiale du lecteur JavaScript public peut prendre plusieurs secondes à sa première utilisation ou lors d'un changement de version. Elle est anticipée en arrière-plan. Une modification de YouTube ou un format non pris en charge peut nécessiter le secours yt-dlp ; les temps de téléchargement restent dépendants du réseau et du serveur audio.
- La reprise après une nuit réelle, les pertes réseau prolongées et les périphériques de casque physiques restent à valider séparément. Les commandes Windows ont été exercées via la session système réelle ; la pause pendant le chargement a été vérifiée.
- La qualité dépend des formats que YouTube autorise pour le compte et le morceau. Un débit mesuré légèrement supérieur à 256 kbit/s reste compatible avec son palier nominal 256 ; l'interface affiche le débit fourni par les métadonnées du flux.
- Le cache audio compressé est borné et aucun fichier musical n'est créé par le nouveau moteur. D'anciens dossiers `%TEMP%\tonal-audio-*` peuvent provenir d'anciennes versions interrompues. Aucun benchmark complet de RAM/CPU du processus n'a encore été réalisé.
- La fermeture libère le contrôleur WebView2 ; l'arrêt de ses sous-processus est asynchrone. Tonal ne tue aucun processus Edge partagé. Le nettoyage du profil temporaire est réessayé pendant cinq secondes, sans garantie si un autre processus le verrouille durablement.
- Les API non officielles et yt-dlp sont soumis aux droits d'utilisation des contenus et aux conditions de YouTube.

Si Cargo échoue avec **4551**, vérifier les événements Code Integrity et Smart App Control dans Sécurité Windows. Ce blocage système est distinct d'une erreur Rust. Voir la [FAQ Microsoft](https://support.microsoft.com/en-gb/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions). Le code et les scripts de Tonal ne changent pas les protections Windows.

## Licence

Code original sous MIT (voir LICENSE). Aucun code de ytm-rs ou tubefast n'a été copié. Références consultées : [ytm-rs](https://github.com/grapedjuice/ytm-rs), sous GPL-3.0, et [tubefast](https://github.com/yigitbozyaka/tubefast), sous MIT. La documentation de [ytmusicapi](https://ytmusicapi.readthedocs.io/en/stable/setup/browser.html) a servi à vérifier le principe des sessions web et des requêtes natives.

Licence du décodeur libopus embarqué : [assets/licenses/libopus.txt](assets/licenses/libopus.txt). L'icône de file est un dessin SVG original ; les autres icônes de commandes conservent leurs attributions existantes.
