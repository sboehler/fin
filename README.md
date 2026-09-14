# fin

A Rust implementation (work in progress) of https://github.com/sboehler/knut.

## Importers

```
fin import ch.postfinance --account Assets:PostFinance:Checking statement.csv
```

```
fin import com.interactivebrokers \
  --account Assets:IBKR --dividend Income:Dividends --interest Expenses:Interest \
  --fee Expenses:Fees --tax Expenses:WithholdingTax --trading Expenses:Trading \
  --rounding Expenses:Rounding activity.csv
```

```
fin import ch.cashback-cards --account Liabilities:CashbackCard statement.csv
```

```
fin import ch.viac --commodity VIAC --portfolio 1.234.567.890.01 summary.json
```

VIAC is valued rather than booked: the importer emits one `price` directive
per day from the portfolio's wealth series. Open app.viac.ch, select "From
start" in the overview dashboard, and save the response of the `summary` XHR
call. Omit `--portfolio` to value the account total instead of a single
portfolio.

Cash amounts are booked in cents while the statement reports them with up to
nine decimals; the difference accumulated over the period is booked to the
`--rounding` account at the period end, so the generated cash assertions hold
given correct opening balances.

Counter-postings without a known account (bank statement lines, broker
deposits and withdrawals) are booked against `Expenses:TBD`; replace that
account when reconciling.

## Inferring accounts

`infer` replaces those `Expenses:TBD` placeholders with a guess, using a naive
Bayes model trained on transactions which already have their accounts
assigned:

```
fin infer --training-file journal/main.journal --inplace imported.journal
```

The training file's includes are followed, and it may be the target file
itself — the usual flow is to append the imported transactions to the journal
and then infer in place. Each booking is described by the words of its
description plus its commodity, quantity, and the account on the other side;
the account with the highest posterior wins. `--account` picks a different
placeholder (default `Expenses:TBD`). Without `--inplace` the result goes to
stdout.

The guesses are only as good as the training data, so check the result — this
rewrites the file through the formatter, so `git diff` shows exactly what
changed.

## Golden tests

Importers are tested against golden files in `testdata/<root>/<importer>/<case>/`
(see `tests/golden.rs`). Each case holds `case.yaml` with the `fin import`
command line to run, e.g. `args: [ch.postfinance, --account, Assets:Bank, statement.csv]`
(file arguments are relative to the case directory), the input file(s) it
names, and `expected.journal`. Regenerate the expected output with

```
UPDATE_GOLDEN=1 cargo test --test golden
```

There are two roots:

- `testdata/public`: mock cases, checked into this repository.
- `testdata/private`: real bank statements, kept in the **private** git
  submodule [`fin-testdata`](https://github.com/sboehler/fin-testdata). Never
  put real statements under `testdata/public`. If the submodule is not
  initialized, the private cases are simply skipped:

  ```
  git submodule update --init
  ```
