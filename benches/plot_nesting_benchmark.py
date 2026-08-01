#!/usr/bin/env python3
"""Renders benches/results.csv into the nesting-depth benchmark chart used
in the docs and README. Usage: plot_nesting_benchmark.py [csv] [out.png]
"""

import csv
import statistics
import sys
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt

REPO_ROOT = Path(__file__).resolve().parent.parent
CSV_PATH = Path(sys.argv[1]) if len(sys.argv) > 1 else REPO_ROOT / "benches" / "results.csv"
OUT_PATH = (
    Path(sys.argv[2])
    if len(sys.argv) > 2
    else REPO_ROOT / "docs" / "assets" / "benchmark-nesting.png"
)

# Fixed categorical order and validated palette slots (see the dataviz
# skill's palette.md): slot 1 blue, 2 green, 3 magenta, 4 yellow, 5 aqua,
# 6 orange -- re-validated as a set of 6 (`validate_palette.js`) before
# adding shadowenv/mise/zsh-autoenv. Magenta/yellow/aqua sit below 3:1
# contrast on the light surface, so every series gets a direct
# end-of-line label rather than relying on color/legend alone.
SERIES = [
    ("easyenv", "#2a78d6"),
    ("direnv", "#008300"),
    ("autoenv", "#e87ba4"),
    ("shadowenv", "#eda100"),
    ("mise", "#1baf7a"),
    ("zsh-autoenv", "#eb6834"),
]

SURFACE = "#fcfcfb"
INK_PRIMARY = "#0b0b0b"
INK_MUTED = "#898781"
GRIDLINE = "#e1e0d9"
BASELINE = "#c3c2b7"


def load_data(csv_path: Path) -> dict[str, dict[int, list[float]]]:
    data: dict[str, dict[int, list[float]]] = {}
    with csv_path.open(newline="") as f:
        for row in csv.DictReader(f):
            tool = row["tool"]
            depth = int(row["depth"])
            ms = int(row["nanoseconds"]) / 1_000_000
            data.setdefault(tool, {}).setdefault(depth, []).append(ms)
    return data


def main() -> None:
    data = load_data(CSV_PATH)
    depths = sorted({d for per_tool in data.values() for d in per_tool})
    x_positions = list(range(len(depths)))

    fig, ax = plt.subplots(figsize=(9, 5.5), dpi=150)
    fig.patch.set_facecolor(SURFACE)
    ax.set_facecolor(SURFACE)

    for tool, color in SERIES:
        per_depth = data.get(tool)
        if not per_depth:
            continue
        medians = [statistics.median(per_depth[d]) for d in depths]
        mins = [min(per_depth[d]) for d in depths]
        maxs = [max(per_depth[d]) for d in depths]
        yerr_low = [m - lo for m, lo in zip(medians, mins)]
        yerr_high = [hi - m for hi, m in zip(maxs, medians)]

        ax.errorbar(
            x_positions,
            medians,
            yerr=[yerr_low, yerr_high],
            fmt="o-",
            color=color,
            linewidth=2,
            markersize=5,
            capsize=3,
            elinewidth=1,
            alpha=0.95,
            label=tool,
        )
        # Direct end-of-line label (required relief for the magenta series,
        # and clearer than legend lookup for the others too).
        ax.annotate(
            tool,
            xy=(x_positions[-1], medians[-1]),
            xytext=(8, 0),
            textcoords="offset points",
            va="center",
            ha="left",
            color=color,
            fontsize=11,
            fontweight="bold",
        )

    ax.set_yscale("log")
    ax.set_xticks(x_positions)
    ax.set_xticklabels([str(d) for d in depths])
    ax.set_xlim(x_positions[0] - 0.4, x_positions[-1] + 1.3)

    ax.set_xlabel("Directory nesting depth (ancestor directories with a config file)", color=INK_MUTED)
    ax.set_ylabel("Time to load on cd (ms, log scale)", color=INK_MUTED)
    ax.set_title(
        "Cold-load latency vs. directory nesting depth",
        color=INK_PRIMARY,
        fontsize=14,
        fontweight="bold",
        loc="left",
    )

    ax.grid(True, which="major", axis="y", color=GRIDLINE, linewidth=0.8, zorder=0)
    ax.grid(True, which="minor", axis="y", color=GRIDLINE, linewidth=0.4, zorder=0)
    ax.set_axisbelow(True)

    for spine_name in ("top", "right"):
        ax.spines[spine_name].set_visible(False)
    for spine_name in ("left", "bottom"):
        ax.spines[spine_name].set_color(BASELINE)

    ax.tick_params(colors=INK_MUTED, labelcolor=INK_MUTED)

    legend = ax.legend(loc="upper left", frameon=False, fontsize=9, labelcolor=INK_PRIMARY)
    for handle in legend.legend_handles:
        handle.set_alpha(1.0)

    fig.tight_layout()
    OUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(OUT_PATH, facecolor=SURFACE)
    print(f"wrote {OUT_PATH}")


if __name__ == "__main__":
    main()
