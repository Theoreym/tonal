# Icône Tonal

`tonal.svg` est le dessin original : un **t minuscule blanc sur fond noir**.

Les fichiers `tonal.png` (256 × 256) et `tonal.ico` (16, 20, 24, 32, 48, 64, 128 et 256 pixels) sont générés depuis ce SVG :

```powershell
cargo run --example export_icon --features ui-capture
```

L'icône SVG est utilisée par la fenêtre native. `build.rs` incorpore l'ICO aux ressources Windows de l'exécutable via le SDK Windows. Le code et cette icône originale sont sous la licence MIT du projet. Les logos des services ont leurs propres conditions, décrites dans [brands/README.md](brands/README.md).

`radios-cover.svg` est la couverture originale de la collection Radios : cadran de fréquences minimal avec graduations claires et repère rouge sur fond sombre. Dessin vectoriel local sous la licence du projet, partagé par la carte et l'en-tête, rendu en une texture de 512 pixels avec mipmaps. Les logos NRJ et Skyrock restent sur leurs stations respectives.
