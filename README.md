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

```
fin import ch.truewealth --commodity TRUEWEALTH evolution.json
```

True Wealth is valued the same way, one `price` directive per day from the
portfolio's end-of-day value. Open app.truewealth.ch, show the performance
chart over the portfolio's full history, and save the response of the
`evolution` XHR call. The export names the currency it reports in, so unlike
VIAC it is not assumed to be francs.

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

A booking is only rewritten if the winner takes at least `--min-confidence`
of the posterior (default `0.9`); otherwise the placeholder is left in place.
A transaction unlike anything in the training data leaves every candidate
tied on the evidence and decided by the prior alone, which would otherwise
produce a confident-looking wrong account rather than something you can grep
for.

To choose a threshold, cross-validate on your own journal:

```
fin infer --evaluate --training-file journal/main.journal
```

```
 coverage   threshold   accuracy  correct/answered
   100.0%    0.136930      83.3%       12929/15530
    80.0%    0.929146      91.3%       11345/12424
    60.0%    0.999196      95.4%         8893/9318
```

`coverage` is how often the model answers, `accuracy` how often those answers
are right, and `threshold` the `--min-confidence` which produces that
coverage. The table is indexed by coverage rather than by confidence so that
two runs stay comparable: changing the features moves the whole confidence
scale, so the same threshold means different things before and after, while
the same coverage does not.

The model is symmetric, so it can predict either side of a booking from the
other, but only one of those is the job a placeholder asks it to do; the two
directions are therefore reported apart rather than averaged. Read the
income/expenses table.

The confidence comes from a naive Bayes posterior, which is overconfident by
construction: most predictions land very close to 1, which is why the
reported thresholds run out to `0.999999`. It separates "no evidence" from
"some evidence" well, but is not a calibrated probability, and the default of
`0.9` is unlikely to be the right operating point for a real journal.

The guesses are only as good as the training data, so check the result — this
rewrites the file through the formatter, so `git diff` shows exactly what
changed.

## Reviewing accounts

`review` turns the same model on the journal itself, and reports the bookings
whose account disagrees with what the rest of the journal suggests:

```
fin review --only-income-expenses journal/main.journal
```

```
journal/postfinance/2024.knut:755  1.0000
  2024-06-11 "LASTSCHRIFT ... Leben // Gesundheit"
  Expenses:Leben:Sonstige:Cash -> Expenses:Leben:Gesundheit:Arzt
```

Each booking is predicted by a model trained on the other folds of the
journal, so a transaction never gets to teach the model its own accounts.
Both sides are questioned but a booking is reported at most once.

A disagreement is a candidate, not a verdict: the model is only right about
95% of the time even when confident, so most findings on a large journal are
the model's mistakes rather than yours. Raise `--min-confidence` to see fewer
and better ones, and prefer `--only-income-expenses` — without it the list
fills with transfers between your own accounts, where the model has no way of
knowing whose card a payment settles.

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
