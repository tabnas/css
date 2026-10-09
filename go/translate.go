/* Copyright (c) 2026 Richard Rodger, MIT License */

package tabnascss

import _ "embed"

// TranslationPart is one optional alchemy source and the entry point a host calls.
type TranslationPart struct {
	Entry  string
	Source string
}

// TranslationParts is the package-local structural translation interface.
type TranslationParts struct {
	Manifest string
	Lift     *TranslationPart
	Embed    *TranslationPart
	Render   *TranslationPart
}

//go:embed translate/manifest.json
var translationManifest string

//go:embed translate/render.alc
var translationRender string

var translationParts = TranslationParts{
	Manifest: translationManifest,
	Render:   &TranslationPart{Entry: "css-render", Source: translationRender},
}

// Translate returns CSS's immutable translation parts: the manifest, whose
// translate object says CSS is read as and written from the reader's own
// tree (the schema css-ast, a stylesheet object at the root), and the
// render, css-render, which writes that tree back as CSS text. There is no
// embed, so a host composes a translation into CSS only from CSS itself or
// from a program that builds the tree.
func Translate() *TranslationParts { return &translationParts }
