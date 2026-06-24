---
name: fluxion-tauri-app-guard
description: Tauri desktop app guard for Fluxion. Use when implementing or reviewing the macOS Fluxion App, Tauri commands, frontend state, task list UI, task detail UI, create-download flows, settings, event subscriptions, or App/Core DTO boundaries.
---

# Fluxion Tauri App Guard

## Required Inputs

Read:

- `.agents/requirements.md` section 6
- `.agents/module-design.md` section 10
- `.agents/development-plan.md` milestones M1-M5

Use frontend design guidance when building visible UI.

## App Boundary Rules

- Interact with Core through Tauri commands and event subscriptions.
- Do not reimplement task lifecycle logic in the frontend.
- Do not access protocol engine internals from UI.
- Keep DTOs serializable and stable.
- Use frontend polling only for initial load or recovery, not as the main progress mechanism.

## Required Screens

First HTTP-focused release needs:

- Downloads task list.
- Task detail view or drawer.
- New HTTP download dialog.
- Settings view.
- Error and empty states.

Task list must show:

- name
- state
- progress
- speed
- size
- remaining time when available
- primary action for current state

## New HTTP Task Form

Support:

- URL.
- save directory.
- optional filename.
- custom headers.
- cookie or credential fields through a secure path.
- max connections, default 16.
- task download limit.
- system proxy choice.

Validate before submission and surface Core validation errors without losing user input.

## UI Quality Rules

- Build the actual task manager UI as the first screen, not a marketing page.
- Keep operational UI dense, calm, and scannable.
- Avoid nested cards and decorative page sections.
- Use icons for common actions where appropriate.
- Ensure button text, filenames, URLs, and errors do not overflow on narrow windows.
- Verify major UI changes in a browser or Tauri preview when possible.

## Completion Checklist

- Commands match Core API semantics.
- Events update UI state without duplicate tasks or stale progress.
- Sensitive fields are masked and not persisted in frontend local storage.
- Task actions are disabled when invalid for the current state.
- UI has loading, error, empty, active, paused, failed, and completed states.

