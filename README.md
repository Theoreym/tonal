<p align="center">
  <img src="assets/tonal.png" width="88" height="88" alt="Logo Tonal">
</p>

<h1 align="center">tonal.</h1>

<p align="center">
  <strong>Ta musique. Une application Windows native.</strong><br>
  Un lecteur YouTube Music en Rust, avec une interface noire et une bibliothèque au premier plan.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-0.3-202020?style=flat-square" alt="Version 0.3">
  <img src="https://img.shields.io/badge/Windows-10%20%2F%2011-202020?style=flat-square" alt="Windows 10 et 11">
  <img src="https://img.shields.io/badge/Rust-natif-202020?style=flat-square" alt="Application native Rust">
  <a href="LICENSE"><img src="https://img.shields.io/badge/licence-MIT-202020?style=flat-square" alt="Licence MIT"></a>
</p>

<p align="center">
  <a href="#fonctionnalités">Fonctionnalités</a> ·
  <a href="#installation">Installation</a> ·
  <a href="#développement">Développement</a> ·
  <a href="docs/ROADMAP.md">Feuille de route</a>
</p>

---

Tonal rassemble tes playlists YouTube Music, des suggestions liées à tes écoutes et deux radios en direct dans une interface sans barre latérale. Le lecteur reste accessible en bas de la fenêtre, avec les pochettes, les commandes essentielles et une progression rouge.

L'interface utilise **egui / OpenGL** et la lecture audio est native. **WebView2 intervient uniquement dans une fenêtre temporaire de connexion Google.**

**Statut :** version 0.3 en développement. YouTube Music et les radios sont disponibles ; Spotify reste désactivé et marqué **SOON** dans les paramètres.

## Fonctionnalités

### Ta bibliothèque

- Retrouve tes playlists, leurs pochettes et leurs morceaux, chargés progressivement.
- Filtre les playlists ou les titres affichés depuis la barre de recherche.
- Crée une playlist publique ou privée ; utilise le clic droit pour la modifier ou la supprimer.
- Actualise ta bibliothèque sans perdre les listes déjà affichées.

### Un lecteur complet

- Lecture, pause, précédent, suivant, répétition et déplacement dans un morceau.
- File de lecture réorganisable : déplacer les suivants, lire un titre immédiatement ou le retirer.
- Aléatoire sur l'ensemble de la playlist ; option **Aléatoire intelligent** pour espacer les artistes et les titres récents.
- **Normalisation du volume** facultative, selon les mesures fournies par YouTube, sans réencodage.
- Restauration de la file et de la position au prochain lancement, en pause.
- Commandes multimédias Windows et masquage facultatif dans la zone de notification.

### Des découvertes à ta demande

Les suggestions partent du morceau écouté. Elles écartent les titres déjà présents dans les playlists modifiables du compte et permettent d'ajouter une découverte à l'une de ces playlists depuis le lecteur.

### NRJ et Skyrock en direct

La carte **Radios**, à la suite des playlists, ouvre une collection de **2 stations**. Elles fonctionnent sans compte Google et utilisent le même lecteur : pastille rouge pulsante, barre rouge pleine et commandes adaptées au direct. Mettre en pause coupe le flux ; reprendre rejoint le direct actuel.

## Installation

**Configuration :** Windows 10 version 2004 ou ultérieure, ou Windows 11 ; architecture x64 ; connexion Internet.

Consulte les [Releases](https://github.com/Theoreym/tonal/releases) pour les paquets publiés. Si aucun paquet n'est disponible, tu peux [compiler le projet](#développement).

- **Installateur** — `tonal-0.3.0-windows-x64-setup.exe` : installation pour le compte Windows courant. WebView2 est proposé s'il manque.
- **Version portable** — `tonal-0.3.0-windows-x64.zip` : extraire l'archive, conserver les fichiers et sous-dossiers ensemble, puis ouvrir `tonal.exe`.

Les paquets incluent les outils de secours : **Rust et les outils de développement ne sont pas nécessaires pour utiliser Tonal.**

Tonal n'est pas encore signé avec un certificat de publication. Windows peut afficher un avertissement ou bloquer son exécution selon sa configuration. Le [guide rapide](docs/QUICKSTART.md) détaille l'installation, les données locales et les contrôles Windows.

## Premiers pas

1. Ouvre le menu de compte en haut à droite et lance la connexion Google.
2. Connecte-toi dans la fenêtre Google temporaire ; Tonal affiche ensuite ta bibliothèque.
3. Ouvre une playlist et sélectionne un titre. Sa pochette et ses informations apparaissent immédiatement dans le lecteur, pendant la préparation du son.
4. Retrouve les options de lecture et les informations de compte dans les **Paramètres**.

Tu peux aussi ouvrir **Radios** et écouter une station sans te connecter.

**Raccourcis :** `Espace` pour lecture / pause, `Ctrl` + `←` / `→` pour précédent / suivant, `Ctrl` + `↑` / `↓` pour le volume et `Ctrl` + `F` pour la recherche. Ils laissent la priorité aux champs de saisie et aux menus ouverts.

## Qualité audio

Tonal lit les flux **Opus ou AAC** autorisés par YouTube pour le compte et le morceau. Avec un compte Premium, les formats autour de **256 kbit/s** sont prioritaires lorsqu'ils sont disponibles. Sinon, le lecteur utilise les formats standard.

Le flux est décodé localement, sans réencodage. Le badge **S / H / M**, activable dans les paramètres, indique un palier de débit ; ce n'est pas une certification studio ou lossless. Le temps de démarrage dépend notamment du réseau et de la préparation du lecteur YouTube.

## Données et connexion

- La session Google et la reprise de lecture sont **chiffrées par Windows DPAPI**, pour le compte Windows courant.
- Les données locales sont conservées dans `%LOCALAPPDATA%\Tonal` : session, reprise, préférences et préparation publique du lecteur YouTube.
- L'audio est mis en tampon en mémoire ; **aucun morceau n'est enregistré sur disque**. Les liens audio signés restent également en mémoire.
- Tonal ne lit pas le champ de mot de passe Google. La connexion s'effectue dans la page Google, via un profil WebView2 temporaire.
- **Déconnexion** efface la session locale ; elle ne révoque pas l'accès depuis les paramètres du compte Google. La désinstallation conserve les données locales.

## Développement

### Prérequis

- [Rust stable](https://rustup.rs/), cible Windows **MSVC x64**.
- Visual Studio Build Tools avec la charge **Développement Desktop en C++** et le SDK Windows.
- **CMake**, accessible dans le `PATH`, pour compiler libopus.
- Microsoft Edge **WebView2 Runtime**, pour essayer la connexion Google.

**Node.js et yt-dlp** dans le `PATH` permettent le chemin de secours. Le chemin principal utilise QuickJS embarqué. FFmpeg sert uniquement à certains essais techniques.

### Lancer Tonal

```powershell
git clone https://github.com/Theoreym/tonal.git
cd tonal
.\scripts\run.ps1
```

Le script compile en mode release puis lance l'application. Depuis un dossier déjà présent, seule la dernière commande est nécessaire. Ferme complètement Tonal avant de relancer après une modification, y compris depuis la zone de notification si le masquage est activé.

L'exécutable est généré dans `target\release\tonal.exe`. La première compilation est plus longue, car elle construit les dépendances.

### Vérifier les changements

```powershell
cargo fmt -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --locked
```

Les tests qui nécessitent le réseau, une session Google ou une sortie audio sont ignorés par défaut. Les scénarios, commandes et résultats sont décrits dans le [journal de validation](docs/VALIDATION.md).

### Préparer une distribution

Le script [scripts/package.ps1](scripts/package.ps1) génère l'installateur, le ZIP autonome et leurs empreintes SHA-256 dans `dist/`. Il nécessite notamment Inno Setup, Node.js, yt-dlp et les fichiers redistribuables MSVC ; le chemin d'Inno Setup peut être fourni avec `-InnoCompiler`.

Les dossiers `target/` et `dist/` sont exclus du dépôt, ainsi que les fichiers de session chiffrés et les fichiers `.env`.

### Repères dans le code

- **Interface** — `main.rs`, `ui.rs`, `window_chrome.rs`, `backdrop.rs`.
- **Bibliothèque et playlists** — `app_state.rs`, `provider.rs`, `playlist_management.rs`, `suggestions.rs`.
- **Audio** — `audio.rs`, `native_stream.rs`, `http_audio.rs`, `player_solver.rs`, `radio.rs`.
- **Intégration Windows** — `desktop.rs`, `native_theme.rs`.
- **Persistance** — `session.rs`, `playback_store.rs`, `preferences.rs`.

Les requêtes réseau et le décodage s'exécutent hors du fil de l'interface. La sortie audio utilise rodio, avec Symphonia pour l'AAC et libopus pour l'Opus ; QuickJS et EJS résolvent les signatures YouTube, avec yt-dlp en secours.

## Documentation et retours

- [Guide rapide](docs/QUICKSTART.md) — installation, utilisation et relance après modification.
- [Feuille de route](docs/ROADMAP.md) — travaux réalisés et validations restantes.
- [Journal de validation](docs/VALIDATION.md) — essais techniques, mesures et limites observées.
- [Signaler un problème](https://github.com/Theoreym/tonal/issues) — indique la version de Tonal, la version de Windows et les étapes pour reproduire. Ne joins pas de cookies, de fichiers de session ou de liens audio signés.

L'intégration YouTube Music repose sur des interfaces non officielles : des changements côté Google peuvent affecter la connexion ou la lecture. Certains parcours de connexion et scénarios de reprise restent à valider ; la feuille de route en précise l'état.

## Licence et crédits

Le code original de Tonal est sous [licence MIT](LICENSE).

[ytm-rs](https://github.com/grapedjuice/ytm-rs) et [Tubefast](https://github.com/yigitbozyaka/tubefast) ont servi de références ; aucun code de ces projets n'a été copié. Tonal utilise notamment egui, rodio, Symphonia, libopus, QuickJS et les icônes Lucide.

Les composants tiers et les marques conservent leurs licences et conditions : [logos](assets/brands/README.md), [icônes](assets/icons/README.md), [solveur EJS et QuickJS](assets/ejs/README.md), [libopus](assets/licenses/libopus.txt). Les paquets incluent également les notices des outils distribués, dont yt-dlp.

Tonal est un projet indépendant, non affilié à Google, YouTube, Spotify, NRJ ou Skyrock.
