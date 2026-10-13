# MTG AI

固定した Boros Burn デッキ同士の対戦を扱う、Rust 製ルールエンジンと探索 AI です。日本語 CLI で対戦できます。公開対象はエンジン・bot・CLI と、その実行・検証に必要なファイルです。

対象は 16 種類・60 枚のデッキによるミラーマッチです。Magic 全体や任意のデッキに対応する汎用エンジンではありません。AI は終局結果を使う MCTS と、同梱の小さな学習済み方策モデルを利用します。

## CLI で対戦

Rust 2024 edition 対応のツールチェーンと Python 3.10 以降が必要です。Rust 1.98.1 / Python 3.12 で検証しています。macOS / Linux を対象としています。

```sh
cargo build --release --locked --manifest-path engine/Cargo.toml --bin play-worker
./play-cli
```

番号を入力して操作します。`s` は全体状況、`q` または Ctrl-D は終了です。

```sh
./play-cli --seed 4 --starting 1 --budget-steps 800000
```

## 構成

| パス | 内容 |
| --- | --- |
| `engine/src/` | ルール、観測、探索、対戦ワーカー、組み込み方策モデル |
| `engine/tests/` | ルール・探索・非公開情報・操作の回帰テスト |
| `research/full_state/src/` | エンジンが依存する状態表現 |
| `cli/` | 日本語 CLI と固定入出力による回帰テスト |

## テスト

```sh
cargo test --release --locked --manifest-path engine/Cargo.toml
cargo test --release --locked --manifest-path research/full_state/Cargo.toml
python3 -m unittest discover -s cli/tests -p 'test_*.py'
```

Python の実行時依存は標準ライブラリだけです。`play-worker` は CLI が使用する対戦ワーカーです。

## 公開範囲

このリポジトリは公開用に選別したソースから始まる独立した履歴です。GUI、以前の開発履歴、未整理の実験、生成レポート、実際の対戦ログ、クラウド運用情報、外部リポジトリのコピーは含みません。CLI の固定データは回帰テスト用として残し、承認記録に含まれていた会話の引用は除いています。

`vendor/` に置かれていた XMage 関連プロジェクトや Forge などのコード・配布物、カード画像、外部カードデータベースは同梱していません。ビルド時の Rust 依存は `Cargo.lock` で管理します。

Magic: The Gathering は Wizards of the Coast の商標です。本プロジェクトは非公式であり、同社との提携・承認を示すものではありません。
