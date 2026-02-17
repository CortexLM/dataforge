# BibliothecaDAO/eternum-4225 (original PR)

BibliothecaDAO/eternum (#4225): landing: re-enable non-cartridge wallet providers

This PR re-enables additional wallet providers in the landing app by keeping recommended Starknet wallets (ArgentX and Braavos) enabled even when injected wallets are present.
It extracts connector option and connector-list composition into `starknet-connectors.ts` and updates `StarknetProvider` to use that shared logic.
It also adds regression tests to ensure recommended providers remain enabled and non-Cartridge connectors are included when `onlyCartridge` is false.
Verified with `pnpm --dir client/apps/landing dlx vitest run src/components/providers/starknet-connectors.test.ts`.
