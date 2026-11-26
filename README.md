# HT Editor

Éditeur photo local pour retoucher une image : ajustements, filtres, recadrage, rotation, dessin et texte. Tout reste sur la machine ; rien n’est envoyé en ligne.

## Lancer l’application

Il faut [Rust](https://rustup.rs/) installé.

```bash
cargo run --release
```

On peut aussi ouvrir un fichier dès le départ :

```bash
cargo run --release -- chemin/vers/photo.jpg
```

Formats acceptés : PNG, JPEG, GIF, BMP et WebP. Un GIF animé n’ouvre que sa première image.

## Ouvrir une image

- **Ouvrir…** ou `Ctrl+O`
- glisser-déposer un fichier dans la fenêtre

Sans image, le panneau de droite reste inactif.

## Se déplacer dans l’image

L’outil **Main** (`V`) sert à naviguer :

- molette : zoom centré sur le curseur
- glisser : déplacer l’image (bouton du milieu dans les autres outils)
- double-clic : ajuster à la fenêtre
- barre du bas : zoom, taille réelle, ajuster à la fenêtre (`Ctrl+0`)

## Outils

La barre du haut propose quatre outils.

| Outil | Touche | Usage |
| --- | --- | --- |
| Main | `V` | Déplacer et zoomer |
| Recadrer | `C` | Glisser une zone, puis **Appliquer** ou `Entrée`. `Échap` annule. |
| Dessin | `B` | Pinceau ou gomme. La gomme rend les pixels transparents. `Ctrl+Z` annule le trait. |
| Texte | `T` | Écrire dans le panneau, cliquer sur l’image pour placer, glisser pour déplacer, puis **Appliquer**. **Centrer** place le texte au milieu. |

Le recadrage, le dessin et le déplacement du texte attendent que le redressement soit appliqué.

## Panneau de droite

Les réglages s’affichent en aperçu. Ils ne sont écrits dans l’image qu’après **Appliquer**.

**Ajustements** — luminosité, contraste, saturation, vignette et grain. Un double-clic sur un curseur le remet à zéro.

**Filtres** — Aucun, Noir & blanc, Sépia, Négatif, Flou léger, Netteté. Recliquer le filtre actif, ou **Retirer**, revient à l’image sans filtre. **Appliquer** le grave (annulable ensuite avec `Ctrl+Z`).

**Transformations** — redressement de −45° à +45° (aperçu, puis **Appliquer** ou `Entrée`, `Échap` pour annuler), rotations −90° / +90° / 180°, miroir horizontal et vertical.

**Édition → Restaurer l'original** revient à l’image telle qu’elle a été ouverte.

## Comparer avant / après

Le bouton **◐** (ou **Affichage → Avant / après**) coupe l’image en deux. Glisser la barre change la séparation : l’original à gauche, la version retouchée à droite.

Maintenir `\` affiche l’original en plein cadre, le temps que la touche reste enfoncée.

## Enregistrer

**Enregistrer** (`Ctrl+S`) ouvre une boîte de dialogue.

- Un fichier **PNG** est écrit tout de suite et conserve la transparence.
- Un fichier **JPEG** ouvre un réglage de qualité (1–100, 90 par défaut) avec une taille estimée. `Entrée` confirme, `Échap` annule. Les pixels transparents sont aplatis sur du blanc.

**Fichier → Exporter JPEG…** force un export JPEG, même si le fichier d’origine est un PNG.

La pastille de la barre d’état passe au orange tant que des modifications ne sont pas enregistrées. L’historique d’annulation garde une trentaine d’étapes (`Ctrl+Z`, `Ctrl+Y` ou `Ctrl+Maj+Z`).

## Raccourcis

| Action | Raccourci |
| --- | --- |
| Ouvrir | `Ctrl+O` |
| Enregistrer | `Ctrl+S` |
| Annuler / rétablir | `Ctrl+Z` / `Ctrl+Y` |
| Ajuster à la fenêtre | `Ctrl+0` |
| Outils Main, Recadrer, Dessin, Texte | `V` `C` `B` `T` |
| Valider recadrage, redressement ou export JPEG | `Entrée` |
| Annuler ces mêmes actions | `Échap` |
| Voir l’original | maintenir `\` |
| Quitter | **Fichier → Quitter** |
