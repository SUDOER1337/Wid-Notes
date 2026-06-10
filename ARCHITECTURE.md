# Project Architecture

> **Note:** This document describes the long-term vision for RustNote. Sections marked with **status: implemented** reflect current v0.1 capabilities. Unmarked sections are aspirational and describe planned architecture for future versions.

## Project Vision

A local-first knowledge workspace written in Rust and egui.

The goal is NOT to create another note-taking application.

The goal is to create a personal workspace where:

- Markdown files are the source of truth.
- Vim is the primary interaction model.
- Local AI agents are first-class citizens.
- Notes, journals, tasks, and AI share a common graph.
- Everything works offline.
- No cloud account.
- No plugin ecosystem.
- No marketplace.
- No telemetry.

This software is intentionally opinionated.

---

# Non Goals

The project will NOT become:

- Notion clone
- Obsidian clone
- Roam clone
- Electron application
- Team collaboration platform
- SaaS product
- Plugin marketplace
- Web application

Any feature that moves the project toward those goals should be rejected.

---

# Core Principles

## Files Are Truth

Everything exists as files.

No proprietary storage.

No hidden database required.

Workspace must remain usable even if the application disappears.

---

## Journal First

Opening the application should immediately open today's journal page.

The journal is the default capture mechanism.

Organization happens later through links.

---

## Local First AI

AI is a workspace component.

AI is never required.

The application must remain fully functional without AI.

---

## Vim First

Vim is not a plugin.

Vim is part of the core interaction model.

Every editor component respects modal editing.

---

# Workspace Layout (v0.1 — implemented)

```
workspace/
├── journals/          # daily journals (YYYY_MM_DD.md)
├── pages/             # user pages (.md)
└── workspace.toml     # name, created_at
```

### Future additions

```
workspace/
├── assets/            # images, attachments
├── templates/         # page templates
├── agents/            # AI agent configs
└── .git/              # version control
```

---

# Markdown Rules

Pages are stored as markdown.

Example:

pages/rust.md

# Rust

My notes.

[[Borrow Checker]]

[[Ownership]]

---

# Link Syntax (future)

Planned link types — none implemented in v0.1.

[[Page]]

#tag

((block-id))

The following will never be supported:

- custom markdown extensions
- proprietary syntax

---

# Architecture

```
Application
├── UI Layer          (implemented: egui panels, menus, widget)
├── Workspace Layer   (implemented: workspace/mod.rs + config.rs)
├── Editor Layer      (implemented: editor/mod.rs — no egui)
├── Vim Layer         (implemented: vim/mod.rs + key.rs — no egui)
├── Preview Layer     (implemented: preview.rs — pulldown-cmark)
├── Graph Layer       (future: page/block graph)
├── Search Layer      (future: full text search)
├── Git Layer         (future: commit, diff, revert)
└── AI Layer          (future: local agents)
```

Dependencies only flow downward. UI never directly touches files.

---

# Module Breakdown

## crate::workspace **— implemented**

Responsible for:

- opening workspaces
- creating workspaces
- workspace metadata
- page/journal CRUD

No UI code.

---

## crate::editor **— implemented**

Responsible for:

- buffer management (text, cursor, selection)
- cursor movement (words, paragraphs, line edges)
- text objects (inner/outer word, paragraph, pair, quote)
- bracket matching
- undo snapshots

Must remain independent from egui.

---

## crate::vim **— implemented**

Responsible for:

- modes (Normal, Insert, Visual, CommandLine)
- operator-motion composition (d{motion}, y{motion}, c{motion})
- text object dispatch (i{, a(, i", etc.)
- registers and clipboard
- repeat (`.`)
- undo / redo
- ex commands (`:w`, `:q`, `:e`, `:help`, etc.)
- search (`/pattern`, `?pattern`)

Must remain independent from egui.

---

## crate::preview **— implemented**

Responsible for:

- pulldown-cmark → egui rendering
- heading, paragraph, code block, list rendering

---

## crate::markdown **— future**

Responsible for:

- parsing markdown
- block extraction
- link extraction
- tag extraction

No filesystem access.

---

## crate::graph **— future**

Responsible for:

- page graph
- block graph
- backlinks
- references

Pure in-memory structures.

---

## crate::search **— future**

Responsible for:

- full text search
- page search
- tag search

Prefer ripgrep integration.

---

## crate::git **— future**

Responsible for:

- commit
- diff
- status
- revert

No UI code.

---

## crate::ai **— future**

Responsible for:

- provider abstraction
- retrieval
- context assembly
- agent execution

Must never depend on UI.

---

# AI Architecture

AI is modeled as agents.

Example:

Research Agent

Writer Agent

Journal Reflection Agent

Project Agent

Each agent has:

- name
- model
- prompt
- memory
- permissions

---

# AI Providers

Provider trait:

- Ollama
- llama.cpp
- future providers

The rest of the application should never know which provider is active.

---

# Retrieval Pipeline

Agent Request

↓

Current Page

↓

Linked Pages

↓

Recent Journals

↓

Search Results

↓

Prompt Assembly

↓

Model

↓

Response

No direct "chatbot" architecture.

Agents operate on workspace context.

---

# MVP (v0.1 — current)

Implemented:

- workspace creation and opening
- journal-first workflow (auto-create today's journal)
- vim modal editing (Normal, Insert, Visual, CommandLine)
- vim motions (h/j/k/l, w/b/e, W/B/E, 0/$/^, gg/G, {/}, %, f/F/t/T, ;/,)
- vim operators with motion composition (d3w, yip, ci(, etc.)
- text objects (iw, aw, ip, ap, i(/a(, i[/a[, i{/a{, i"/a", i'/a')
- visual mode (charwise, linewise) with d/y/c/~/></
- count support on motions and operators
- registers ("a-"z, unnamed, small-delete)
- undo / redo (200-level stack)
- repeat (`.`)
- search (/pattern, ?pattern)
- ex commands (:w, :q, :wq, :q!, :wq!, :e, :help, :noh)
- markdown preview via pulldown-cmark
- sidebar page and journal list
- Ctrl+S save, Ctrl+f/Ctrl+b scroll

Not yet implemented (planned for v0.2+):

- page links ([[Page]])
- backlinks
- git integration
- command palette (search)
- graph visualization
- AI agents
- whiteboards
- sync
- mobile support

---

# Version 0.2

Add:

- local AI provider
- embeddings
- retrieval
- agent panel

---

# Version 0.3

Add:

- graph visualization
- block references
- workspace dashboards

---

# Version 1.0

Requirements:

- stable workspace format
- stable markdown compatibility
- stable git workflow
- fully offline operation
- documented architecture

No feature may be added after 1.0 without a clear use case.
Complexity is a bug.
