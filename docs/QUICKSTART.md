# tonal v0.3

Windows 10 2004 ou plus récent / Windows 11, processeur x64 et connexion Internet.

## Installer

Ouvrir `tonal-0.3.0-windows-x64-setup.exe`. L'installation concerne le compte Windows courant et ne demande pas les droits administrateur. Le composant Microsoft WebView2 est installé si nécessaire pour la connexion Google ; cette étape peut télécharger des fichiers depuis Microsoft.

La version ZIP peut aussi être extraite dans un dossier au choix. Garder ensemble `tonal.exe`, les deux DLL et les dossiers `bin` et `licenses`, puis ouvrir `tonal.exe`. Si le composant de connexion Google manque, lancer `bin/MicrosoftEdgeWebview2Setup.exe`.

Aucun besoin d'installer Rust, Python, Node.js, yt-dlp ou FFmpeg. Les outils de secours sont inclus. L'interface et la lecture restent natives.

## Utiliser

Dans Bibliothèque, la carte Radios apparaît à la suite des playlists. L'ouvrir affiche ta collection, avec NRJ et Skyrock au départ. Le bouton + ouvre une recherche par nom ; cliquer sur + à droite d'un résultat ajoute cette station. Une coche indique les stations déjà ajoutées. Un clic droit sur une station de la collection permet de la retirer. La collection est conservée dans `%LOCALAPPDATA%\Tonal\radios.json`, même après redémarrage ou déconnexion de Google. Le catalogue Radio Browser n'est interrogé qu'après saisie d'une recherche ; seuls les résultats bornés et leurs visuels visibles sont chargés. La recherche est limitée aux flux MP3 directs compatibles, hors HLS.

Cliquer sur une station lance le direct dans le lecteur habituel, sans connexion Google. La pause coupe la connexion et la reprise rejoint le direct actuel. Une pastille rouge pulsante remplace la durée ; la barre rouge reste pleine et non déplaçable. Les boutons précédent, suivant, répétition, aléatoire et file sont inactifs pendant une radio ; le volume reste réglable.

Sous le nom de la station, Tonal affiche artiste — titre quand le flux fournit un couple exploitable. Les noms d'émission reconnus et les identifiants numériques sont masqués. Les métadonnées viennent directement de la radio, sans reconnaissance audio ; leur disponibilité et leur fraîcheur dépendent de la station. À défaut, « Radio en direct » reste affiché.

Se connecter à Google depuis le bouton Compte, choisir une playlist, puis un titre. La qualité dépend de l'abonnement et des formats disponibles. La recherche filtre la bibliothèque personnelle.

Le bouton `+` près du rafraîchissement crée une playlist avec le titre choisi et le switch Visibilité activé par défaut ; le désactiver crée une playlist privée. Un clic droit sur une playlist affiche immédiatement Modifier / Supprimer. La confidentialité existante est récupérée en arrière-plan avant d'autoriser sa modification. Ces actions sont synchronisées avec YouTube Music et réservées aux playlists modifiables par le compte ; la suppression demande une confirmation.

- Espace : lecture / pause.
- Ctrl + gauche / droite : précédent / suivant.
- Ctrl + haut / bas : volume, par pas de 5 %.
- Ctrl + F : recherche dans la bibliothèque.
- Échap : fermer les paramètres ou le menu actif.

Les commandes de lecture ne capturent pas les touches lorsqu'on saisit du texte ou qu'un menu est ouvert.

Tonal suit la sortie audio par défaut de Windows. Si Windows choisit tes écouteurs après leur connexion, le lecteur transfère le son vers eux sans sélecteur à régler dans Tonal. Le titre, la position, le volume et la pause sont conservés ; un court silence peut accompagner le changement de sortie. Connecter un périphérique qui ne devient pas la sortie par défaut ne change pas la lecture.

## Relancer après une modification du code

Fermer complètement Tonal, y compris son icône dans la zone de notification si l'option de masquage est activée. Dans un terminal PowerShell ouvert à la racine du projet, lancer :

```powershell
.\scripts\run.ps1
```

Le script retrouve Rust et CMake, compile les changements en mode release, puis ouvre la nouvelle version. La compilation peut prendre environ une minute lorsque le code a changé ; sans changement, le lancement est rapide.

## Données

Session et reprise sont chiffrées pour le compte Windows courant dans `%LOCALAPPDATA%\Tonal`. Les préférences et la préparation publique du lecteur y sont aussi conservées. Aucun morceau n'est enregistré sur disque. La désinstallation conserve ces données ; utiliser Déconnexion dans Tonal pour effacer la session locale.

## Protection Windows

Cette version de Tonal n'est pas signée avec un certificat de publication. SmartScreen peut afficher « application non reconnue », et Smart App Control ou une politique d'entreprise peut bloquer son exécution. Un installateur n'annule pas ces contrôles. Vérifier la provenance et les empreintes du paquet ; aucune protection Windows n'est modifiée par Tonal.

Documentation Microsoft : https://learn.microsoft.com/windows/apps/package-and-deploy/smartscreen-reputation
