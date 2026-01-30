# Contributing Guide

Thank you for your interest in contributing to this repository!  
We welcome improvements, bug fixes, documentation updates, and security feedback.

Please read this guide before submitting a contribution.

---

## Repository Overview

This repository contains a Solana program (Lockbox) and related deployment and interaction scripts.

Main technologies:
- Solana
- Rust (on-chain program)
- TypeScript (client scripts)
- solana-cli

---

## Development Setup

### Prerequisites

Ensure the following tools are installed:

- Rust (stable)
- Solana CLI
- Node.js
- npm or yarn

Install Solana CLI:
```bash
sh -c "$(curl -sSfL https://release.solana.com/stable/install)"
```

Verify installation:
```bash
solana --version
```

---

## Building the Program

To build the Solana program:

```bash
cargo build-bpf
```

Or use the provided deployment scripts in the `scripts/` directory.

---

## Deployment

Deployment helpers are provided in `scripts/`.

Example:
```bash
./scripts/deploy.sh
```

Please review deployment scripts carefully before executing them, especially on mainnet.

---

## Client Scripts

TypeScript scripts for interacting with the program are located in `scripts/`.

Install dependencies:
```bash
npm install
```

Example usage:
```bash
ts-node scripts/lockbox_init.ts
```

---

## Code Style

- Follow standard Rust formatting:
```bash
cargo fmt
```

- Keep on-chain code minimal and explicit
- Avoid unnecessary complexity in Solana programs

TypeScript:
- Prefer clear, explicit logic
- Avoid magic constants without explanation

---

## Security

This project contains on-chain financial logic.

When contributing:
- Avoid unsafe assumptions
- Be explicit about account validation
- Clearly document any changes affecting fund flow
- If you find a potential vulnerability, consider responsible disclosure

Audit documents can be found in the `doc/` directory.

---

## Pull Request Guidelines

When submitting a Pull Request:

- Explain the purpose of the change
- Keep PRs small and reviewable
- Do not mix refactors with functional changes
- Update documentation when needed
- Ensure the program builds successfully

---

## Commit Messages

Use clear, descriptive commit messages.

Recommended format:
```
<type>: short description
```

Examples:
- `fix: validate token account ownership`
- `feat: add withdrawal instruction`
- `docs: update deployment instructions`

---

## Reporting Issues

Please open an issue for:
- Bugs
- Security concerns
- Improvement suggestions

Provide as much context as possible.

---

## Commit Messages & Branching

- Use **Conventional Commits**:
    - `feat: ...`, `fix: ...`, `docs: ...`, `refactor: ...`, `test: ...`, `chore: ...`, `perf: ...`
- Branch names:
    - `feat/<short-topic>`, `fix/<short-topic>`, `docs/<short-topic>`
- Reference issues/PRs in the body (e.g., `Closes #123`).

> Optionally enforce **DCO** (`Signed-off-by`) or a **CLA** as part of CI.

---

## License & CLA/DCO

- This project is licensed under **MIT**. See `LICENSE`.
- If required, contributors must sign a **CLA** or use **DCO** sign-offs. Document the process in this section or link to your CLA portal.
- By contributing, you agree that your contributions will be licensed under the same license as the project.

---

## Contact

- General questions: **info@valory.xyz**
- Security: **security@valory.xyz**