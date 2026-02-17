import re
import unittest
from pathlib import Path

CONFIG_PATH = Path("package/batocera/core/batocera-system/Config.in")


def parse_selects(path: Path) -> dict[str, list[str | None]]:
    selects: dict[str, list[str | None]] = {}
    for raw_line in path.read_text().splitlines():
        line = raw_line.split("#", 1)[0].strip()
        if not line:
            continue
        match = re.match(r"^select\s+(\S+)(?:\s+if\s+(.+))?$", line)
        if match:
            symbol = match.group(1)
            condition = match.group(2)
            selects.setdefault(symbol, []).append(condition)
    return selects


def eval_expr(expr: str | None, symbols: dict[str, bool]) -> bool:
    if expr is None:
        return True
    normalized = expr.replace("&&", " and ").replace("||", " or ")
    normalized = re.sub(r"!(?!=)", " not ", normalized)

    def replace_symbol(match: re.Match[str]) -> str:
        token = match.group(0)
        if token in {"and", "or", "not"}:
            return token
        return f"symbols.get('{token}', False)"

    normalized = re.sub(r"\b[A-Za-z0-9_]+\b", replace_symbol, normalized)
    return bool(eval(normalized, {"symbols": symbols}))


def is_selected(symbol: str, selects: dict[str, list[str | None]], symbols: dict[str, bool]) -> bool:
    if symbol not in selects:
        return False
    return any(eval_expr(condition, symbols) for condition in selects[symbol])


def compute_config(selects: dict[str, list[str | None]], base_symbols: dict[str, bool]) -> dict[str, bool]:
    symbols = dict(base_symbols)
    symbols["BR2_PACKAGE_YQUAKE2"] = is_selected("BR2_PACKAGE_YQUAKE2", selects, symbols)
    return symbols


class TestYQuake2RiscvSelection(unittest.TestCase):
    def test_yquake2_selection_respects_riscv_flag(self) -> None:
        selects = parse_selects(CONFIG_PATH)

        non_riscv_config = compute_config(selects, {"BR2_riscv": False})
        self.assertTrue(non_riscv_config["BR2_PACKAGE_YQUAKE2"])

        riscv_config = compute_config(selects, {"BR2_riscv": True})
        self.assertFalse(riscv_config["BR2_PACKAGE_YQUAKE2"])

    def test_yquake2_mission_packs_follow_selection(self) -> None:
        selects = parse_selects(CONFIG_PATH)

        non_riscv_config = compute_config(selects, {"BR2_riscv": False})
        self.assertTrue(is_selected("BR2_PACKAGE_YQUAKE2_XATRIX", selects, non_riscv_config))

        riscv_config = compute_config(selects, {"BR2_riscv": True})
        self.assertFalse(is_selected("BR2_PACKAGE_YQUAKE2_XATRIX", selects, riscv_config))


if __name__ == "__main__":
    unittest.main()
