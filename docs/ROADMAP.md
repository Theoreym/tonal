# TONAL — feuille de route

## Contraintes conservées

- Windows, Rust, interface native egui / OpenGL.
- Noir absolu `#000000`, cartes `#141414`, texte `#FFFFFF`, secondaire `#A1A1AA`, accent `#FF4444`.
- YouTube Music prioritaire. Spotify reste désactivé ; le bloc des services est masqué à la demande de l'utilisateur.
- WebView2 limité à une fenêtre temporaire de connexion ; aucune WebView pour la bibliothèque ou la lecture.
- Réduire RAM/CPU sans nouvelle compression audio avec perte.

## Étape 1 — stabiliser l'alpha

Réalisé le 11 novembre 2026 :

- [x] Installer Rust x64 MSVC, Cargo, les outils C++ et le SDK Windows.
- [x] Compiler et corriger l'alpha ; conserver Cargo.lock.
- [x] Exécuter les contrôles Rust et produire `target/release/tonal.exe`.
- [x] Tester WAV, MP3, FLAC, OGG sur la sortie audio Windows : pause, reprise, arrêt, rechargement et fin de titre.
- [x] Installer yt-dlp et vérifier FFmpeg ; tester une URL publique jusqu'à la lecture native et au nettoyage du WAV.
- [x] Extraire AppState, Provider et StreamResolver dans leurs modules.
- [x] Refuser les URL HTTP et les hôtes non autorisés ; ignorer la configuration personnelle yt-dlp.
- [x] Rafraîchir l'interface en fin de titre, désactiver les commandes de lecture à vide, gérer les fichiers temporaires.
- [x] Interface native centrée, bibliothèque en cartes, logos officiels et lecteur fixe ; menu compte et paramètres en haut à droite. Barre latérale, fichiers locaux et saisie d'URL retirés à la demande de l'utilisateur.
- [x] Icône « t » minuscule intégrée à la fenêtre et à l'exécutable ; barre de fenêtre native intégrée au thème noir, recherche de bibliothèque, compte, commandes Windows et bordures de redimensionnement.
- [x] Arrondi natif Windows 11 via DWM ; boutons harmonisés, menu compte et états de survol/focus retravaillés.
- [x] Paramètres modaux avec flou GPU, compte et statut Premium explicite, Spotify SOON désactivé ; croix, clic extérieur et Échap. Thème fixe et volume dans le lecteur uniquement. Badge de débit S / H / M facultatif et préférence persistante ; anticrénelage des icônes adapté à la densité de pixels.
- [x] Migration vers egui / eframe 0.37 : texte avec hinting Skrifa / Vello CPU, logos SVG rendus par taille / DPI, lettres S / H / M homogènes. Contrôles du lecteur à identifiants stables pendant l'apparition du volume ; traitement de la file dans `App::logic` pour les fenêtres masquées.
- [x] Lecteur : visuel avec marges égales, titre en gras, artiste réel, commandes principales centrées et progression sur la séparation supérieure.
- [x] Recherche alignée sur la loupe et sélection des textes statiques désactivée globalement, pour conserver le survol des lignes sur leurs titres.
- [ ] Écoute subjective et validation des interactions par l'utilisateur.
- [x] Scénario reproductible de mémoire privée et CPU : visible, lecture, pause et masquage, puis stabilisation ; rapport JSON et échantillons CSV. Mesures sur quelques minutes, pas une validation de fuite sur plusieurs jours.
- [x] Distribution Windows v0.3 : ZIP autonome et installateur par utilisateur, outils de secours inclus et empreintes vérifiées, DLL C++ app-local, détection / installation WebView2 si absent. Version Cargo et métadonnées Windows 0.3.0.

## Étape 2 — connexion et bibliothèque YouTube Music

Préparation implémentée :

- [x] Fenêtre WebView2 avec profil isolé temporaire, navigation principale limitée à Google / YouTube en HTTPS. Mode normal pour ne pas imposer les restrictions de clés d'accès de la navigation privée.
- [x] Annulation et expiration de la fenêtre de connexion ; fermeture explicite et libération du contrôleur.
- [x] Extraction des cookies uniquement après retour sur `music.youtube.com` ; aucun accès au champ mot de passe.
- [x] DPAPI pour le compte Windows courant ; aucun fichier de session en clair ; déconnexion locale.
- [x] Vérification serveur de la session avant d'afficher un état connecté.
- [x] Backend HTTP natif Innertube : playlists de bibliothèque, titres et pagination à la demande.
- [x] Conserver les listes pendant leur actualisation ; cache de métadonnées borné pour la réouverture des playlists, remplacement cohérent des pages, anciennes suggestions conservées sur erreur et transitions courtes limitées aux lignes visibles.
- [x] Pochettes réelles de playlists et morceaux, album / durée dans les lignes ; cache natif borné PNG / JPEG / WebP.
- [x] Photo et prénom issus du compte Google dans le bouton et le menu Compte ; playlists automatiques masquées.
- [x] Pagination automatique au défilement, état du titre en cours et survol des pochettes avec Play sur dégradé ; messages techniques de succès et compteurs retirés.
- [x] Version du client web lue sur YouTube Music, délais réseau et taille des réponses bornés, redirections authentifiées refusées.
- [x] Gestion des sessions expirées, révocation, erreurs réseau et réponses non reconnues.
- [x] Tests des parseurs, de DPAPI et du rejet d'une session factice ; deux cycles réels WebView2 ouverts puis annulés et nettoyés.
- [x] Corriger le conflit de classes Windows entre Winit et Tao ; tester deux connexions annulées avec la fenêtre native ouverte dans le même processus.

À valider avant de déclarer la connexion prête :

- [x] Connexion réelle et première bibliothèque confirmées par l'utilisateur après correction du transfert régional `accounts.google.fr` (11 novembre 2026).
- [ ] Valider séparément le parcours clé d'accès et les autres méthodes 2FA. Si Google refuse WebView2, afficher ce refus sans chercher à le contourner.
- [ ] Vérifier une bibliothèque réelle vide et une bibliothèque de plusieurs pages ; compléter les fixtures anonymisées avec l'accord de l'utilisateur.
- [ ] Vérifier l'ouverture et la pagination des titres d'une vraie playlist, puis le redémarrage et la restauration DPAPI.
- [ ] Vérifier l'expiration/révocation réelle, la déconnexion pendant une requête et la fermeture de Tonal pendant le login.
- [ ] Vérifier l'absence de processus WebView2 appartenant à Tonal après une authentification réussie, en idle et pendant la lecture. Les cycles annulés ont été vérifiés sans processus résiduel.
- [ ] Gérer la sélection de compte / chaîne et les évolutions du schéma Innertube.
- [x] Suggestions liées au titre courant via Provider : accès volontaire depuis la bibliothèque, 12 propositions maximum, pochettes et file indépendante ; pas de recherche globale ni de paroles.
- [ ] Favoris synchronisés via Provider.
- [x] Ajout explicite du titre suggéré à une playlist YouTube Music modifiable, menu dans le lecteur à droite de la file, vérification de présence et protection des doublons.
- [x] Ajout optimiste : notification blanche en haut à droite, fermeture immédiate du menu, compteurs / listes mis à jour, restauration sur erreur et relecture serveur en fond. Titres déjà présents exclus des suggestions lors du chargement et après les ajouts.
- [x] Création de playlist depuis le bouton + près du rafraîchissement ; titre et confidentialité privée / publique. Menu au clic droit pour modifier ou supprimer avec confirmation ; droits vérifiés, écritures en arrière-plan, notifications et caches actualisés, lecture conservée.

La résolution des flux réutilise maintenant la session Google : les formats autorisés par YouTube sont demandés avec le compte, puis l'audio est lu nativement sans cookies sur le CDN.

## Étape 3 — audio natif progressif

Architecture intégrée le 11 novembre 2026 après étude de Tubefast et ytm-rs : blocs HTTP de 256 Kio, cache compressé borné à 8 Mio, décodage Opus / AAC en Rust, thread de décodage séparé de la sortie rodio non bloquante, préchargement des premiers 512 Kio du titre suivant. Aucun WAV ni FFmpeg dans le chemin de lecture. Depuis le 12 novembre, Innertube et QuickJS embarqué assurent la résolution des signatures ; yt-dlp reste le secours. Un lien proche de son expiration ou refusé en 403 / 410 peut être renouvelé à format et taille identiques, avec une seule tentative par lecture de bloc. Plusieurs expirations sur un même flux ont été simulées avec de vrais renouvellements réseau ; le fonctionnement après longue pause paraît satisfaisant selon l'utilisateur au 13 novembre.

Sources examinées : [Tubefast player.rs](https://github.com/yigitbozyaka/tubefast/blob/e5c39a9ce3f6f8494b58aa61e78827adc4cd5c33/src/player.rs), [Tubefast Cargo.toml](https://github.com/yigitbozyaka/tubefast/blob/e5c39a9ce3f6f8494b58aa61e78827adc4cd5c33/Cargo.toml), [ytm-rs stream.rs](https://github.com/grapedjuice/ytm-rs/blob/5e896a90496ef4f9f651fb81afeb2473cfa78678/src/stream.rs) et [backend.rs](https://github.com/grapedjuice/ytm-rs/blob/5e896a90496ef4f9f651fb81afeb2473cfa78678/src/backend.rs). Aucun code de ces projets n'a été intégré aux sources de Tonal ; les fichiers consultés localement restent dans `target/audio-research/`.

- [x] Remplacer la préparation complète WAV par une source progressive dans StreamResolver, en conservant Opus/AAC.
- [x] Cache borné, préchargement du prochain titre, renouvellement borné des URL expirées / refusées et meilleure qualité autorisée (774 / 141 Premium, puis 251 / 140 standard).
- [x] Annulation des préparations yt-dlp, délai global de 75 secondes et fermeture de leur arbre de processus propre.
- [x] Pause réseau : annulation des requêtes audio et du préchargement suivant, fermeture du pool HTTP, suspension du décodage ; reprise à la position conservée avec le cache courant.
- [x] Reprise après longue pause : flux frais et sortie Windows rouverte au même endroit, titre / file / volume conservés ; Lecture reste utilisable après une erreur. Nouvelles tentatives espacées pour les pochettes en échec.
- [ ] Confirmer avec l'utilisateur une reprise après une nuit réelle, notamment après mise en veille Windows et changement de périphérique audio.
- [ ] Confirmer avec l'utilisateur l'absence d'avertissement de lecture simultanée dans YouTube après mise en pause de Tonal.
- [x] Affichage immédiat des métadonnées du titre choisi, indépendant du chargement ; un autre clic annule le travail précédent.
- [x] Vérifier la lecture native réelle : AAC 141 avec la session enregistrée, Opus 251 sans session, pause et déplacement avant / arrière.
- [x] Vérifier l'annulation par une nouvelle sélection et la consommation du flux préchargé du prochain titre sur une vraie playlist.
- [ ] Tester une expiration réelle d'URL pendant lecture et les pertes réseau prolongées.
- [x] Affichage de la position, de la durée disponible et commande muet.
- [x] File des titres déjà chargés, suivant/précédent, enchaînement automatique, aléatoire et répétition playlist / titre.
- [x] Progression rouge, durée près du volume, grandes icônes arrondies et slider de volume révélé au survol.
- [x] Déplacement dans le titre par clic ou glissement, point rouge au survol, conservation de la pause.
- [x] Étendre la file quand les nouvelles pages affichées sont chargées, sans changer le titre courant.
- [x] Aléatoire sur le catalogue complet de la playlist, chargé par pages en arrière-plan sans défilement, sans audio ni pochettes supplémentaires ; historique réel et doublons préservés, annulation et erreur sans repli silencieux sur une page partielle.
- [x] Lecture séquentielle sans défilement : préparer le catalogue restant par pages à l'approche de la fin de la file connue ; conserver les 100 titres visibles et l'ordre canonique.
- [x] Windows SMTC : métadonnées, pochette, progression, Lecture / Pause / Suivant / Précédent et déplacement, y compris Pause pendant le chargement. Zone de notification facultative et retour à la fenêtre.
- [x] Suivre les changements de sortie audio Windows par défaut via Core Audio, sans sélecteur ; transfert du même lecteur et décodeur, conservation du volume / pause / métadonnées, retries bornés pendant l'initialisation du périphérique.
- [x] Reprise chiffrée de la file / position / volume / shuffle / répétition, restauration en pause et sans préparation audio, effacement par Arrêt / Déconnexion.
- [x] Panneau central temporaire de file : glisser les suivants, lire maintenant, retirer sans modifier la playlist YouTube ; état de file conservé dans la reprise.
- [x] Chemin HTTP natif et résolution des signatures / `n` dans QuickJS embarqué avec EJS officiel ; repli yt-dlp Music puis multi-client, qualité Premium conservée.
- [x] Cache en mémoire des liens résolus, isolé par session, borné à 32 entrées / 20 minutes et protégé contre l'expiration ; renouvellement forcé sans cache et normalisation parallèle au repli yt-dlp.
- [x] Réduire le délai du premier lancement des liens chiffrés : moteur QuickJS isolé, préparation publique en arrière-plan, cache de source par version, conservation de la qualité Premium et secours yt-dlp.
- [x] Remplacer le décodeur Opus expérimental par libopus après un échec observé sur un flux Premium réel ; conserver le flux original et le décodage local, sans transcodage.
- [x] Mesurer mémoire privée et CPU moyen sur Windows : repos, lecture, pause, masquage et stabilisation. Les paroles ne font pas partie du produit actuel.

## Références et risques

- [ytm-rs](https://github.com/grapedjuice/ytm-rs) : GPL-3.0. Aucune source copiée.
- [tubefast](https://github.com/yigitbozyaka/tubefast) : MIT lors de la consultation. Aucune source copiée.
- [Sessions web ytmusicapi](https://ytmusicapi.readthedocs.io/en/stable/setup/browser.html) et documentation Wry / Microsoft WebView2 pour les interfaces techniques.
- Innertube est non officiel ; Google peut refuser WebView2 et modifier les endpoints. Ne pas appeler cette intégration « OAuth prêt ».
- Les données déjà compressées ne peuvent pas devenir lossless par conversion.
- WebView2 ferme son contrôleur, puis ses sous-processus de manière asynchrone ; ne jamais tuer les processus Edge d'autres applications.

## Prochain ticket

- [x] Section Radios dans Ta bibliothèque : NRJ et Skyrock, logos locaux, flux MP3 natifs, pause coupant le réseau et reprise au direct. Décodage réel, changement de station sur sortie muette et captures validés le 16 novembre.
- [x] Collection de radios personnalisable : bouton +, recherche Radio Browser à la demande, ajout sans doublons, retrait au clic droit et persistance locale indépendante de Google. Catalogue complet jamais chargé ; flux MP3 directs compatibles.

- [x] Aléatoire intelligent facultatif dans les paramètres : espacement des artistes et titres récents, conservation du courant / historique / doublons / exclusions.
- [x] Normalisation facultative dans les paramètres : atténuation par mesure YouTube, volume utilisateur inchangé, pas d'amplification ni de transcodage.

**Suite : affiner les suggestions selon l'usage. Mini-lecteur à réévaluer. Mesurer RAM / CPU et poursuivre la réduction du délai de résolution des liens chiffrés.**

Voir [VALIDATION.md](VALIDATION.md) pour les commandes et les résultats effectivement observés.
