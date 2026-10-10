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
// render, css-render, which writes that tree back as CSS text. The render
// takes a node's members in any order, so the plain maps Parse returns,
// which a host walks in sorted key order, are written as the TypeScript and
// Rust trees are. There is no embed, so a host composes a translation into
// CSS only from CSS itself or from a program that builds the tree.
// Each call returns a copy of its own, so that what one caller changes
// is not what another reads.
func Translate() *TranslationParts {
	parts := translationParts
	parts.Lift = copyPart(parts.Lift)
	parts.Embed = copyPart(parts.Embed)
	parts.Render = copyPart(parts.Render)
	return &parts
}

// copyPart is a part of its own, so that no caller reaches another's.
func copyPart(part *TranslationPart) *TranslationPart {
	if part == nil {
		return nil
	}
	copied := *part
	return &copied
}
