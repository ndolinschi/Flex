"""Sum two integers given on the command line."""

import argparse


def main() -> None:
    parser = argparse.ArgumentParser(description="add two integers")
    parser.add_argument("a", type=int)
    parser.add_argument("b", type=int)
    args = parser.parse_args()
    print(f"sum: {args.a + args.b}")


if __name__ == "__main__":
    main()
