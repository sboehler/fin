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
fin import com.schwab \
  --account Assets:Investments:Schwab --transfer Assets:Investments:Schwab:Awards \
  --bank Assets:Bank --dividend Income:Dividends --tax Expenses:WithholdingTax \
  --interest Income:Interest --trading Expenses:Trading --fee Expenses:Fees \
  transactions.csv
```

Under "Accounts / History", pick the account and the date range and export the
transactions as CSV. Transactions are sorted by date; the order within a day
is left as the export has it. `--transfer` receives internal transfers between
Schwab accounts (`Journal`, `Journaled Shares`), `--bank` external ones
(`MoneyLink Transfer`); the latter defaults to `Expenses:TBD`. Cash amounts are reported net of commission, so a trade books
the gross amount against `--trading` and the commission against `--fee`. A
dividend and the withholding tax withheld from it are reported as two rows and
are booked as one transaction.

The Equity Awards Center export is a different layout and has its own command,
which carries none of the cash management flags the brokerage account needs:

```
fin import com.schwab.awards \
  --account Assets:Investments:Schwab:Awards --award Income:Salary:Stock \
  --transfer Assets:Investments:Schwab --trading Expenses:Trading \
  --fee Expenses:Fees awards.csv
```

Here `--account` is the awards account, `--award` the income account the
vested shares are credited to, and `--transfer` the brokerage account the
sale proceeds are journalled to. All deposits of a day form one `Award`
transaction, which the export lists before the sale it funded. Rows without a
date hold the lot details of the row above and are ignored.

Both exports report that journal of the proceeds, once from each side, so
importing both files yields the transfer twice; the two transactions are
identical and one of each pair is meant to be dropped by hand.

Each command checks the header and refuses the other command's export, rather
than reading its columns at the wrong offsets.

```
fin import com.revolut \
  --account Assets:Revolut --fee Expenses:Fees --trading Expenses:Trading \
  chf.csv eur.csv usd.csv
```

Revolut keeps one balance per currency and exports one CSV per currency; open
each account in the app and download its statement. Pass all of them to one
invocation: a currency exchange moves money between two of those balances and
is reported twice, once in each statement, and the two legs are matched into
a single transaction booked against `--trading`. They are recognized by the
time the exchange was started, which both rows carry; a leg whose counterpart
is missing, because the other statement was not passed, is imported on its
own against `Expenses:TBD`. Fees are charged on top of the amount and go to
`--fee`. Rows which never completed, reverted or still pending, are skipped.
English and German exports are both read, and may be mixed.

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
## Charts

```
fin chart sankey journal.fin --valuation CHF -m 2,Expenses -m 2,Assets -o flows.html
```

Renders the flows between accounts over a period as a sankey diagram. Every
booking carries its counter-account, so the journal is already a flow graph;
the chart shows it after netting flows that run both ways between the same
pair of accounts.

The account projection flags are the same as `fin balance`'s: `-m LEVEL,REGEX`
shortens matching accounts to `LEVEL` segments and `-r` substitutes virtual
accounts. `-m` is what makes the chart readable — collapsing the asset
accounts turns the raw graph into an income statement, with the accounts money
actually flowed through as the hub. Use `--min` to prune small flows.

Flows in different commodities cannot be added up, so a journal using more
than one commodity needs `--valuation`. Note that `--valuation` also books
unrealized gains against the valuation account, which then show up as a source
of income in the chart.

Sankey layout requires an acyclic graph. Flows that run both ways between two
accounts are netted into one, and any cycle left after that has its smallest
flow dropped, which is reported on stderr.

The output is a self-contained HTML file: the ECharts bundle in `vendor/`
(Apache-2.0) is inlined, so the chart works offline and keeps working wherever
the file is copied. `--format json` emits just the ECharts option object
instead.

## Golden tests

Importers are tested against golden files in `testdata/<root>/<importer>/<case>/`
(see `tests/golden.rs`). Each case holds `case.yaml` with the `fin import`
command line to run, e.g. `args: [ch.postfinance, --account, Assets:Bank, statement.csv]`
(file arguments are relative to the case directory), the input file(s) it
names, and `expected.journal`. Regenerate the expected output with

```
UPDATE_GOLDEN=1 cargo test --test golden
```

The chart reports have their own cases in `testdata/public/chart/<case>/`, with
a `chart.yaml` manifest and an `expected.json` holding the ECharts option
object (see `tests/chart.rs`); regenerate them the same way with
`UPDATE_GOLDEN=1 cargo test --test chart`.

The importer roots are two:

- `testdata/public`: mock cases, checked into this repository.
- `testdata/private`: real bank statements, kept in the **private** git
  submodule [`fin-testdata`](https://github.com/sboehler/fin-testdata). Never
  put real statements under `testdata/public`. If the submodule is not
  initialized, the private cases are simply skipped:

  ```
  git submodule update --init
  ```
