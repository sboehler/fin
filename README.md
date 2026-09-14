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

Cash amounts are booked in cents while the statement reports them with up to
nine decimals; the difference accumulated over the period is booked to the
`--rounding` account at the period end, so the generated cash assertions hold
given correct opening balances.

Counter-postings without a known account (bank statement lines, broker
deposits and withdrawals) are booked against `Equity:TBD`; replace that
account when reconciling.

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
