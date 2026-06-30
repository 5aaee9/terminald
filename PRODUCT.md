# Product

## Register

product

## Users

Terminald is for small internal engineering teams that need a trusted browser-based terminal for operations, debugging, and remote shell access. Users are developers or operators working in a focused task context where terminal output is the primary content and the surrounding UI should clarify state without competing for attention.

## Product Purpose

Terminald provides a PTY-backed web terminal with a Rust server, a React frontend, Basic authentication, reverse-proxy-friendly paths, and a CLI client. Success means a team member can open a terminal session, understand its connection and authentication state, interact efficiently, and diagnose failures without guessing whether the issue is auth, WebSocket transport, the remote command, or the network.

## Brand Personality

Reliable, restrained, secure, efficient, modern, and diagnostic. The interface should feel like a serious internal operations tool: quiet by default, explicit when state changes, and polished enough to earn trust without becoming decorative.

## Anti-references

Terminald should not look like a marketing SaaS page with hero sections, promotional copy, or decorative card grids. It should not look like a flashy terminal theme showcase with neon effects, excessive animation, or visual styling that competes with terminal content. It also should not remain a bare xterm demo with only a black screen and rough status text.

## Design Principles

1. Keep the terminal content primary. Chrome, status, and controls should support the shell rather than frame it as a demo.
2. Make state legible. Authentication, connection, reconnecting, remote exit, and error states should be visible, precise, and easy to distinguish.
3. Prefer operational trust over novelty. Use familiar product UI patterns, stable layout, and restrained motion.
4. Reduce uncertainty at failure boundaries. Users should be able to tell whether a failure is caused by credentials, WebSocket upgrade, server process exit, or network loss.
5. Preserve speed of use. The default path should stay keyboard-friendly and low-friction for repeated team operations.

## Accessibility & Inclusion

Use WCAG AA as the baseline. Maintain sufficient contrast for status text and terminal-adjacent UI, preserve keyboard operability, avoid motion that is not tied to state, and respect reduced-motion preferences. Design should remain usable for long-running operations and low-quality internal displays.
