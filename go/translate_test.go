/* Copyright (c) 2026 Richard Rodger, MIT License */

package tabnascss

import (
	"encoding/json"
	"os"
	"strings"
	"testing"
)

// The translation parts are the package's copies, under translate/, of
// tabnas.plugin.json and the files its translate object names; npm run
// embed in ts/ writes them. They are the only texts a host sees, so these
// hold them to the files.

type translateSpec struct {
	Reads  string   `json:"reads"`
	Writes string   `json:"writes"`
	Root   string   `json:"root"`
	Schema string   `json:"schema"`
	Lift   *string  `json:"lift"`
	Embed  *string  `json:"embed"`
	Render string   `json:"render"`
	Loss   []string `json:"loss"`
}

func readTranslateSpec(t *testing.T) translateSpec {
	t.Helper()
	manifest, err := os.ReadFile("../tabnas.plugin.json")
	if err != nil {
		t.Fatal(err)
	}
	var spec struct {
		LanguageID string         `json:"languageId"`
		Translate  *translateSpec `json:"translate"`
	}
	if err := json.Unmarshal(manifest, &spec); err != nil {
		t.Fatal(err)
	}
	if spec.Translate == nil {
		t.Fatal("the manifest carries no translate object")
	}
	if spec.LanguageID != "css" {
		t.Fatalf("languageId is %q", spec.LanguageID)
	}
	return *spec.Translate
}

func TestTranslationParts(t *testing.T) {
	parts := Translate()
	if parts == nil {
		t.Fatal("Translate returned nil")
	}
	manifest, err := os.ReadFile("../tabnas.plugin.json")
	if err != nil {
		t.Fatal(err)
	}
	if parts.Manifest != string(manifest) {
		t.Fatal("embedded manifest differs from tabnas.plugin.json: run npm run embed in ts")
	}
	if parts.Lift != nil {
		t.Fatal("CSS has no lift")
	}
	spec := readTranslateSpec(t)
	if parts.Render == nil || parts.Render.Entry != "css-render" {
		t.Fatalf("render entry is %#v", parts.Render)
	}
	render, err := os.ReadFile("../" + spec.Render)
	if err != nil {
		t.Fatal(err)
	}
	if parts.Render.Source != string(render) {
		t.Fatalf("embedded render differs from %s: run npm run embed in ts", spec.Render)
	}
}

// An embed takes a plain tree into a format's own schema. CSS's tree is
// the reader's stylesheet, and a plain tree has no CSS form, so its
// manifest names none and the package carries none; a manifest that named
// one would be held to its file here, as the render is above.
func TestTranslationEmbed(t *testing.T) {
	spec := readTranslateSpec(t)
	parts := Translate()
	if spec.Embed == nil {
		if parts.Embed != nil {
			t.Fatalf("the manifest names no embed, and Translate carries %#v", parts.Embed)
		}
		return
	}
	if parts.Embed == nil || parts.Embed.Entry != "css-embed" {
		t.Fatalf("embed entry is %#v", parts.Embed)
	}
	embed, err := os.ReadFile("../" + *spec.Embed)
	if err != nil {
		t.Fatal(err)
	}
	if parts.Embed.Source != string(embed) {
		t.Fatalf("embedded embed differs from %s", *spec.Embed)
	}
}

// CSS is read as and written from its own tree: the schema names it, its
// root is the stylesheet, an object, and the loss lines are sentences, which
// a host prints as they are.
func TestTranslationShapes(t *testing.T) {
	spec := readTranslateSpec(t)
	if spec.Reads != "tree" || spec.Writes != "tree" || spec.Root != "object" || spec.Schema != "css-ast" {
		t.Fatalf("translate object is %#v", spec)
	}
	if spec.Lift != nil {
		t.Fatal("CSS has no lift")
	}
	if len(spec.Loss) == 0 {
		t.Fatal("translate.loss is empty")
	}
	for _, line := range spec.Loss {
		if line == "" || strings.ToUpper(line[:1]) != line[:1] || !strings.HasSuffix(line, ".") {
			t.Fatalf("%q is not a sentence", line)
		}
	}
}
