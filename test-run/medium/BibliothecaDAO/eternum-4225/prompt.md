# BibliothecaDAO/eternum-4225

BibliothecaDAO/eternum (#4225): landing: re-enable non-cartridge wallet providers

Enable recommended Starknet wallet providers (ArgentX and Braavos) in the landing app even when injected wallets are present. Ensure non-Cartridge wallet connectors are available when the configuration does not restrict to Cartridge-only. Add coverage to prevent regressions of these behaviors.
