"""Repeat allocation workloads into separate stage-14-named generated reports."""
import importlib


def main():
    for name in ["phase", "namespace", "inference", "calls", "functions", "dates", "rewrites"]:
        module = importlib.import_module(f"{name}_memory")
        module.main(report_name=f"stage14-{name}-memory.json")


if __name__ == "__main__":
    main()
