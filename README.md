# fin — a plain text accounting tool

WARNING: AI generated README.

[![Test Suite](https://github.com/sboehler/fin/workflows/Test%20Suite/badge.svg)](https://github.com/sboehler/fin/actions)

fin is a double-entry accounting tool for the command line. The ledger is a
plain text file you own and keep in git; fin parses it, values it in any
commodity you have prices for, and reports on it. It is a Rust implementation
(work in progress) of [knut](https://github.com/sboehler/knut), and its use
cases are personal finance and investing.

```
$ fin balance journal.fin --valuation CHF --months --to 2020-03-31
+---------------+------------+------------+------------+------------+
|    Account    | 2019-12-31 | 2020-01-31 | 2020-02-29 | 2020-03-31 |
+---------------+------------+------------+------------+------------+
| Assets        |            |            |            |            |
|   Checking    |     10,000 |     11,800 |     14,127 |     14,127 |
|   Portfolio   |            |      1,032 |        926 |        863 |
|               |            |            |            |            |
| Total (A+L)   |     10,000 |     12,832 |     15,053 |     14,990 |
+---------------+------------+------------+------------+------------+
| Expenses      |            |            |            |            |
|   Rent        |            |     -2,000 |     -2,000 |            |
|   Groceries   |            |       -200 |       -673 |            |
|   Fees        |            |         -4 |            |            |
|               |            |            |            |            |
| Income        |            |            |            |            |
|   Salary      |            |      5,000 |      5,000 |            |
|   Portfolio   |            |         34 |       -106 |        -63 |
|               |            |            |            |            |
| Equity        |            |            |            |            |
|   Equity      |     10,000 |     10,002 |     12,832 |     15,053 |
|               |            |            |            |            |
| Total (E+I+E) |     10,000 |     12,832 |     15,053 |     14,990 |
+---------------+------------+------------+------------+------------+
| Delta         |            |            |            |            |
+---------------+------------+------------+------------+------------+
```

## What it does

- **A booking names both accounts.** A transaction cannot fail to balance,
  and the journal is a flow graph rather than a pile of postings.
- **Valuation in any commodity.** Given prices, fin values every position
  daily, books the unrealized gains, and the balance sheet still balances.
- **Reports over periods.** Daily through yearly columns, cumulative or
  period differences, with accounts collapsed to the depth you want to read.
- **Importers** for nine Swiss and US bank, card and broker exports, which
  turn a downloaded CSV or JSON into journal directives.
- **Account inference.** A naive Bayes model fills in the placeholder
  accounts the importers leave behind, and can be turned on the journal
  itself to find bookings that look misfiled.
- **Sankey charts** of the flows between accounts, as a self-contained HTML
  file that works offline.
- **A formatter**, so the journal has one canonical layout and `git diff`
  shows what actually changed.

## Table of contents

- [Installation](#installation)
- [Tutorial](#tutorial)
- [File format](#file-format)
- [Commands](#commands)
  - [balance](#balance)
  - [chart sankey](#chart-sankey)
  - [import](#import)
  - [infer](#infer)
  - [review](#review)
  - [fetch](#fetch)
  - [format](#format)
  - [migrate](#migrate)
  - [parse](#parse)
- [Development](#development)

## Installation

With cargo:

```
cargo install --path .
```

With nix, which is what the flake in this repository is for:

```
nix run github:sboehler/fin -- balance journal.fin
nix profile install github:sboehler/fin
```

`nix develop` drops you into a shell with the toolchain this repository is
built with.

## Tutorial

### A first journal

A journal is a text file. Create `journal.fin`:

```
* Accounts

2019-12-31 open Equity:Equity
2019-12-31 open Assets:Checking
2019-12-31 open Income:Salary
2019-12-31 open Expenses:Rent
2019-12-31 open Expenses:Groceries

* Opening balances

2019-12-31 "Opening balance"
Equity:Equity        Assets:Checking            10000 CHF

* 2020-01

2020-01-02 "Rent January"
Assets:Checking      Expenses:Rent               2000 CHF

2020-01-15 "Groceries"
Assets:Checking      Expenses:Groceries           200 CHF

2020-01-25 "Salary January"
Income:Salary        Assets:Checking             5000 CHF
```

Every account has to be opened before it is used, and its first segment says
what kind of account it is: `Assets`, `Liabilities`, `Equity`, `Income` or
`Expenses`. Lines starting with `*`, `#` or `//` are comments — `*` is
org-mode's heading marker, so an editor that folds org sections will fold a
journal of any size.

A booking names the account money flows **from**, the account it flows
**to**, and the amount:

```
<credit account> <debit account> <quantity> <commodity>
```

Money flows left to right, so `Income:Salary Assets:Checking 5000 CHF` is
salary arriving in the checking account, and `Assets:Checking
Expenses:Rent 2000 CHF` is rent leaving it. Both accounts are on the same
line, so there is no way to write a transaction that does not balance.

Ask for a balance:

```
$ fin balance journal.fin --quantity
+---------------+------------+
|    Account    | 2026-09-24 |
+---------------+------------+
| Assets        |            |
|   Checking    |            |
|     CHF       |     12,800 |
...
```

`--quantity` reports the quantities held, broken down by commodity. Without
it fin reports *values*, which needs prices — that comes next. The report
ends on today's date unless `--to` says otherwise.

### Positions in other commodities

Add a portfolio funded from the checking account, which buys dollars and then
shares. Append to `journal.fin`:

```
2019-12-31 open Assets:Portfolio
2019-12-31 open Expenses:Fees

2020-01-05 "Transfer to portfolio"
Assets:Checking      Assets:Portfolio            1000 CHF

2020-01-06 "Currency exchange"
Assets:Portfolio     Equity:Equity                969 CHF
Equity:Equity        Assets:Portfolio            1000 USD

2020-01-06 "Buy 12 AAPL"
Assets:Portfolio     Equity:Equity                892 USD
Equity:Equity        Assets:Portfolio              12 AAPL
Assets:Portfolio     Expenses:Fees                  4 USD

* 2020-02

2020-02-02 "Rent February"
Assets:Checking      Expenses:Rent               2000 CHF

2020-02-05 "Groceries"
Assets:Checking      Expenses:Groceries           250 CHF

2020-02-25 "Groceries"
Assets:Checking      Expenses:Groceries           423 CHF

2020-02-25 "Salary February"
Income:Salary        Assets:Checking             5000 CHF
```

An exchange is not a flow between two accounts of yours — one commodity
leaves the world and another enters it — so it is booked against
`Equity:Equity`, twice, once per commodity. The same goes for a purchase.

The portfolio now holds three commodities and the balance cannot be added up
until they are made comparable. Put prices in their own files,
`prices/USD.fin`:

```
2019-12-31 price USD 0.9707 CHF
2020-01-31 price USD 0.9694 CHF
2020-02-29 price USD 0.9689 CHF
2020-03-31 price USD 0.9601 CHF
```

and `prices/AAPL.fin`:

```
2020-01-06 price AAPL 74.33 USD
2020-01-31 price AAPL 77.38 USD
2020-02-29 price AAPL 68.34 USD
2020-03-31 price AAPL 63.57 USD
```

and pull them in at the top of `journal.fin`:

```
include "prices/USD.fin"
include "prices/AAPL.fin"
```

Note that AAPL is priced in USD and USD in CHF: fin chains prices, and
inverts them where needed, so a price of AAPL in francs is not something you
have to provide.

### Valuation

```
$ fin balance journal.fin --valuation CHF --to 2020-03-31
+---------------+------------+
|    Account    | 2020-03-31 |
+---------------+------------+
| Assets        |            |
|   Checking    |     14,127 |
|   Portfolio   |        863 |
|               |            |
| Total (A+L)   |     14,990 |
+---------------+------------+
| Expenses      |            |
|   Rent        |     -4,000 |
|   Groceries   |       -873 |
|   Fees        |         -4 |
|               |            |
| Income        |            |
|   Salary      |     10,000 |
|   Portfolio   |       -135 |
|               |            |
| Equity        |            |
|   Equity      |     10,002 |
|               |            |
| Total (E+I+E) |     14,990 |
+---------------+------------+
| Delta         |            |
+---------------+------------+
```

`Income:Portfolio` was never opened and never booked to: it is the valuation
account of `Assets:Portfolio`, and fin books the day's change in the value of
a position there. That is what keeps the two totals equal, and it is why the
report is an honest income statement rather than a cash-flow statement — the
-135 francs are what the position lost, whether or not anything was sold.

`Delta` is the difference between the two totals. Empty means the report
adds up.

### Periods

`--days`, `--weeks`, `--months`, `--quarters` and `--years` turn the single
column into a series:

```
$ fin balance journal.fin -v CHF --months --to 2020-03-31
+---------------+------------+------------+------------+------------+
|    Account    | 2019-12-31 | 2020-01-31 | 2020-02-29 | 2020-03-31 |
+---------------+------------+------------+------------+------------+
| Assets        |            |            |            |            |
|   Checking    |     10,000 |     11,800 |     14,127 |     14,127 |
|   Portfolio   |            |      1,032 |        926 |        863 |
|               |            |            |            |            |
| Total (A+L)   |     10,000 |     12,832 |     15,053 |     14,990 |
+---------------+------------+------------+------------+------------+
| Expenses      |            |            |            |            |
|   Rent        |            |     -2,000 |     -2,000 |            |
|   Groceries   |            |       -200 |       -673 |            |
|   Fees        |            |         -4 |            |            |
|               |            |            |            |            |
| Income        |            |            |            |            |
|   Salary      |            |      5,000 |      5,000 |            |
|   Portfolio   |            |         34 |       -106 |        -63 |
|               |            |            |            |            |
| Equity        |            |            |            |            |
|   Equity      |     10,000 |     10,002 |     12,832 |     15,053 |
|               |            |            |            |            |
| Total (E+I+E) |     10,000 |     12,832 |     15,053 |     14,990 |
+---------------+------------+------------+------------+------------+
| Delta         |            |            |            |            |
+---------------+------------+------------+------------+------------+
```

Assets are cumulative and income and expenses are per period, which is the
usual reading of a balance sheet. `--diff` reports every row as its change
over the period instead, and `-m LEVEL,REGEX` shortens matching accounts to
`LEVEL` segments, which is how a report gets short enough to read:

```
$ fin balance journal.fin -v CHF --months --from 2020-02-01 --to 2020-03-31 \
    --diff -m '1,(Income|Expenses)'
+---------------+------------+------------+
|    Account    | 2020-02-29 | 2020-03-31 |
+---------------+------------+------------+
| Assets        |            |            |
|   Checking    |      2,327 |            |
|   Portfolio   |       -106 |        -63 |
|               |            |            |
| Total (A+L)   |      2,221 |        -63 |
+---------------+------------+------------+
| Expenses      |     -2,673 |            |
|               |            |            |
| Income        |      4,894 |        -63 |
|               |            |            |
| Total (E+I+E) |      2,221 |        -63 |
+---------------+------------+------------+
| Delta         |            |            |
+---------------+------------+------------+
```

`-m 0,REGEX` drops the matching accounts altogether. What they held then
shows up in `Delta`, so nothing goes missing quietly:

```
$ fin balance journal.fin -v CHF --months --from 2020-02-01 --to 2020-03-31 \
    --diff -m '0,(Income|Expenses)'
+---------------+------------+------------+
|    Account    | 2020-02-29 | 2020-03-31 |
+---------------+------------+------------+
| Assets        |            |            |
|   Checking    |      2,327 |            |
|   Portfolio   |       -106 |        -63 |
|               |            |            |
| Total (A+L)   |      2,221 |        -63 |
+---------------+------------+------------+
| Total (E+I+E) |            |            |
+---------------+------------+------------+
| Delta         |      2,221 |        -63 |
+---------------+------------+------------+
```

Valuation collapses the commodities of an account into one number. Use
`-s REGEX` to keep the accounts it matches broken out, still valued:

```
$ fin balance journal.fin -v CHF --to 2020-03-31 -s Portfolio
...
|   Portfolio   |            |
|     USD       |        100 |
|     AAPL      |        732 |
|     CHF       |         31 |
...
```

### Checking the journal against the bank

A `balance` directive asserts what an account holds on a date. Add one that
is deliberately wrong:

```
2020-01-31 balance Assets:Checking              11801 CHF
```

```
$ fin balance journal.fin -v CHF --to 2020-03-31
Error: balance directive on 2020-01-31: account Assets:Checking has balance 11800 CHF, want 11801 CHF.

Defined in file "journal.fin", line 57, column 20

   57 |2020-01-31 balance Assets:Checking              11801 CHF
```

Copy the closing balance of each statement into the journal as you import it
and the journal cannot drift away from the bank without saying so. Fix the
number to `11800` before continuing.

### Tidying up

```
fin format journal.fin
```

aligns the accounts and the amounts of every transaction, leaving comments
and the whitespace between directives alone. Run it before committing and
diffs stay about the numbers.

### Charting the flows

Every booking names both of its accounts, so the journal is already a graph
of where money went:

```
fin chart sankey journal.fin -v CHF -m 2,Assets -o flows.html
```

`flows.html` is a self-contained page — open it in a browser, copy it
anywhere, no network needed. Collapsing the asset accounts with `-m` is what
makes it readable: income arrives at one hub and expenses leave from it, so
the chart becomes an income statement with the accounts money actually flowed
through in the middle. See [chart sankey](#chart-sankey) for the rest.

### Importing a statement instead of typing it

```
fin import ch.postfinance --account Assets:Checking statement.csv >> journal.fin
```

The importer knows the account the statement belongs to, but not the account
on the other side of each line, so it books those against `Expenses:TBD`.
Rather than assigning them by hand, let the journal's own history do it:

```
fin infer --training-file journal.fin --inplace journal.fin
```

The model learns from the transactions that already have their accounts, and
replaces a placeholder only when it is confident. What it does not fill in
stays `Expenses:TBD`, which is a string you can grep for. Check the result
with `git diff`.

### Keeping prices up to date

Rather than typing prices, describe where they come from, in `prices.yaml`:

```yaml
- commodity: "USD"
  target_commodity: "CHF"
  file: "prices/USD.fin"
  symbol: "USDCHF=X"
- commodity: "AAPL"
  target_commodity: "USD"
  file: "prices/AAPL.fin"
  symbol: "AAPL"
```

```
fin fetch prices.yaml
```

fetches the last year of quotes from Yahoo! Finance and rewrites each file.

That is the whole loop: download statements, import, infer, assert, report.

## File format

A journal is a sequence of directives. Their order in the file does not
matter — they are evaluated by date. Lines starting with `#`, `*` or `//`
are ignored.

### Open and close

An account is a sequence of segments separated by `:`, the first of which
must be `Assets`, `Liabilities`, `Equity`, `Income` or `Expenses`. An account
must be opened before it is used:

```
YYYY-MM-DD open <account>
```

and can be closed once it is no longer needed, which prevents further
bookings. Closing an account whose balance is not zero is an error:

```
YYYY-MM-DD close <account>
```

### Transactions

```
YYYY-MM-DD "<description>"
<credit account> <debit account> <quantity> <commodity>
<credit account> <debit account> <quantity> <commodity>
...
```

A date, a description in double quotes, and one or more bookings on the
lines immediately following. Every booking names a credit account (first)
and a debit account (second); the quantity is usually positive and money
flows from left to right.

This deviates from ledger and beancount, where a posting names one account
and the transaction balances only if the postings happen to sum to zero.
Naming both:

- makes an unbalanced transaction unrepresentable,
- records an unambiguous flow between two accounts, which is what the
  reports and the sankey chart are built on,
- is more compact.

#### The arrow notation

The same transaction can be written with the description unquoted on the
line below the date, and the bookings as groups of accounts joined by
arrows:

```
YYYY-MM-DD
  <description>
  <more description>
<account>
-> <account> <quantity> <commodity>
-> <account> <quantity> <commodity>
```

The description runs over as many indented lines as it needs, and the line
breaks between them are kept — by the reports, and by `fin format`, which
re-indents every line by two spaces. A blank line ends the transaction, so
the description cannot contain one.

One account of a group starts at column zero, the accounts facing it start at
column zero with an arrow. One side is a single account without an amount and
the other lists accounts which each have one, which gives one booking per
amount — so the group above is two bookings out of the account at column
zero. The single account can be either side, so

```
<account> <quantity> <commodity>
<account> <quantity> <commodity>
-> <account>
```

is two bookings into the account behind the arrow.

The arrow points from the credit account to the debit account, and `<-`
reverses that, so a group can collect what flows out of an account and what
flows into it at once:

```
Assets:Bank
-> Expenses:Fees          1.00 CHF
<- Income:Interest        4.20 CHF
```

Each arrow carries the direction of its own booking, and where a group has a
single arrow it gives its direction to every account facing it.

A transaction is one or more such groups, and buying a security is typically
two of them:

```
@performance(VT,USD)
2026-06-24
  Buy 11 VT @ 154.45 USD
Assets:Investments:IBKR
-> Expenses:Investments:Trading    1698.95 USD
-> Expenses:Investments:Fees          1.00 USD
Expenses:Investments:Trading
-> Assets:Investments:IBKR              11 VT
```

Both notations can be mixed in one file, and `fin format` keeps each
transaction in the notation it was written in.

### Balance assertions

```
YYYY-MM-DD balance <account> <quantity> <commodity>
```

Checks that the account holds exactly that on that date, and reports an
error, with the source location, if it does not. Several assertions sharing
a date can be written as a block:

```
2020-01-31 balance
Assets:Checking     11800 CHF
Assets:Portfolio     1000 USD
Assets:Portfolio       12 AAPL
```

### Prices

```
YYYY-MM-DD price <commodity> <price> <target commodity>
```

`2020-01-06 price AAPL 74.33 USD` says one AAPL cost 74.33 USD that day.
Prices chain and invert, so a price of AAPL in USD plus a price of USD in CHF
is enough to value an AAPL position in francs. The most recent price on or
before a day is the one used; valuing a date before the first known price is
an error.

Valuation books the change in the value of a position to the *valuation
account* of the account holding it, which is its name with the first segment
replaced by `Income` — `Assets:Portfolio` gains and loses through
`Income:Portfolio`. Those accounts need no `open` directive.

### Include

```
include "<relative path>"
```

The path is relative to the file the directive appears in. Whether to keep
one large journal or many small ones is a matter of taste; prices in
particular are worth keeping apart, since `fin fetch` rewrites those files.

### Virtual accounts

A virtual account stands in for a set of accounts matching regular
expressions, so a report can aggregate accounts that are not siblings in the
tree:

```
virtual Assets:Liquid
Assets:Checking
Assets:Savings
Liabilities:CreditCard
```

The directive declares the grouping; it takes effect only when a report is
asked for it with `-r`:

```
fin balance journal.fin -v CHF -r Assets:Liquid
```

Substitution happens before `-m`, so the two compose.

### Accruals

`@accrue` annotates a transaction to spread its flows over time. A yearly
bill paid in March otherwise lands entirely in March:

```
@accrue quarterly 2020-01-01 2020-12-31 Assets:Prepaid
2020-03-24 "Insurance 2020"
Assets:Checking Expenses:Insurance 1200 CHF
```

The transaction is replaced by one that moves the money into the accrual
account, plus a series that moves it out into the expense account over the
interval. Amounts are split without remainder and the total impact is
unchanged.

```
@accrue <once|daily|weekly|monthly|quarterly|yearly> <from> <to> <account>
```

### Performance annotations

`@performance(<commodities>)` names the commodities a transaction is about,
for attributing performance. The empty form is the useful one: a custody fee
charged on a portfolio as a whole belongs to no single commodity, and
`@performance()` says so rather than letting it be attributed to whatever it
was paid in.

```
@performance()
2024-03-31 "Custody fee"
Assets:Portfolio Expenses:Fees 12.50 CHF
```

## Commands

### balance

```
fin balance [OPTIONS] <PATH>
```

| Flag | Meaning |
| --- | --- |
| `-v, --valuation <COMMODITY>` | Value all positions in this commodity. Without it no values are known and the columns come out empty, so a report without `-v` needs `-q`. |
| `-q, --quantity` | Report quantities held, per commodity, rather than values. |
| `-s, --show-commodities <REGEX>` | Break the accounts matching this regex down by commodity, keeping the values. Repeatable. |
| `-m, --mapping <LEVEL,REGEX>` | Shorten accounts matching `REGEX` to `LEVEL` segments; `0` drops them into `Delta`. The regex may be omitted (`-m 2`), which matches everything. Repeatable. |
| `-r, --vaccounts <ACCOUNT>` | Substitute a [virtual account](#virtual-accounts) for the accounts it matches. Repeatable. |
| `-f, --from`, `-t, --to <DATE>` | Period covered. Defaults to the first transaction and today. |
| `--days`, `--weeks`, `--months`, `--quarters`, `--years` | One column per period instead of a single column. |
| `--last <N>` | Keep only the last `N` periods, plus the column they start from. |
| `--diff` | Report each row as its change over the period rather than cumulatively. |
| `--round <N>` | Decimal places. The default is 0. |

### chart sankey

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

By default every flow runs straight from one account to the other, so income
accounts all arrive at the same node and expenses all leave from it. `--fan
LEVEL,REGEX` routes matching flows through their `LEVEL`-segment ancestor
instead, which makes the account tree visible: income converges on its
categories before reaching the accounts it lands in, and spending diverges
from them.

```
fin chart sankey journal.fin -v CHF -m3,Income -m3,Expenses \
  --fan 2,Income --fan 2,Expenses
```

turns `Income:Lohn:FirmaA` and `Income:Lohn:FirmaB` into two flows meeting at
`Income:Lohn`, and `Expenses:Wohnen` into one flow splitting into the accounts
below it. Which way a chain points is not something to configure: it follows
from which end of the flow the account hangs off. The flag is repeatable, so
several levels chain.

Flows in different commodities cannot be added up, so a journal using more
than one commodity needs `--valuation`. Note that `--valuation` also books
unrealized gains against the valuation account, which then show up as a source
of income in the chart.

Sankey layout requires an acyclic graph. Flows that run both ways between two
accounts are netted into one, and any cycle left after that has its smallest
flow dropped, which is reported on stderr.

Money that stays in an account is not a flow, so on its own it would simply
stop at the node with nothing to explain the difference. Each asset,
liability or equity account therefore gets an explicit edge for what it
retained over the period, to a synthetic `Net change` node — or from an
`Opening balance` node, for an account that was drawn down instead. The chart
then conserves, and those edges add up to the change in net worth the balance
report shows. Pass `--no-balance` to leave them out. Income and expense
accounts get none: that is where money starts and ends.

The output is a self-contained HTML file: the ECharts bundle in `vendor/`
(Apache-2.0) is inlined, so the chart works offline and keeps working wherever
the file is copied. `--format json` emits just the ECharts option object
instead.

### import

Importers write journal directives to stdout. They are named after the
institution's domain, reversed:

| Command | Export |
| --- | --- |
| `ch.postfinance` | PostFinance CSV account statement |
| `ch.cashback-cards` | Swisscard Cashback Cards statement |
| `ch.swissquote` | Swissquote transactions export |
| `ch.viac` | VIAC portfolio values, JSON |
| `ch.truewealth` | True Wealth portfolio values, JSON |
| `com.interactivebrokers` | Interactive Brokers activity statement |
| `com.revolut` | Revolut CSV statements, one per currency |
| `com.schwab` | Charles Schwab brokerage transactions |
| `com.schwab.awards` | Charles Schwab equity awards |

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
fin import ch.swissquote \
  --account Assets:Investments:Swissquote --dividend Income:Dividends \
  --interest Income:Interest --tax Expenses:WithholdingTax \
  --fee Expenses:Fees --trading Expenses:Trading transactions.csv
```

Open the transactions overview, pick the date range and export it as CSV. The
export is Latin-1 encoded, semicolon separated and German; its `Nettobetrag`
is the cash which moved, reported after the `Kosten` already deducted from
it, so a trade books the gross amount against `--trading` and the fee against
`--fee`, and a dividend the gross payment against `--dividend` and the
withholding tax against `--tax`. A currency exchange is reported as two rows
sharing a timestamp, which are matched into one transaction booked against
`--trading`; an unmatched leg is booked against `--trading` on its own.
Deposits and withdrawals, and rows whose type the importer does not know, go
to `Expenses:TBD`. Custody fees are charged on the portfolio as a whole, so
they are annotated `@performance()`, naming no commodity of their own.

Swissquote keeps one balance per currency, and one assertion per currency is
emitted for the last one the export reports; as with Revolut, those only hold
if the export reaches back to the account's first booking. The rows are
listed newest first and are reordered by their timestamp, which also orders
the rows within a day.

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

### infer

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

### review

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

### fetch

```
fin fetch prices.yaml
```

Fetches the last year of daily quotes from Yahoo! Finance for every entry and
rewrites its file. The config is a list of entries; `file` is relative to the
config:

```yaml
- commodity: "USD"
  target_commodity: "CHF"
  file: "prices/USD.fin"
  symbol: "USDCHF=X"
```

A symbol that cannot be fetched — delisted, misspelled, or a request the API
refused — does not cost the others their quotes: everything that was fetched
is written, and the failures are reported together at the end.

Yahoo restates the prices from before a stock split, so a file fetched before
one disagrees with quotes fetched after it by the split factor, up to the day
the split took effect. `fetch` detects that — a run of prices that all moved
by the same factor, followed by prices that did not — and brings the earlier
prices in the file, the ones the fetch does not reach back far enough to
restate itself, onto the new scale, so the file stays on one scale
throughout. A split is not an error, but it rewrites prices that were not
fetched, so it is reported rather than done quietly.

### format

```
fin format journal.fin prices/USD.fin
```

Formats each file in place, aligning accounts and amounts, including across
the two notations for transactions. Comments and the whitespace between
directives are preserved.

### migrate

```
fin migrate journal.fin journal/*.fin
```

Rewrites the transactions of each file from one booking per line into [the
arrow notation](#the-arrow-notation), in place, and formats the result.
Includes are not followed, so name every file; `--dry-run` prints to stdout
instead of writing, and `--width` (80 by default) says where the description
is wrapped.

Each transaction is written around the account appearing in most of its
bookings, which goes at column zero, with the accounts it receives from
before the accounts it pays, each side sorted by account. The bookings that account is not part of form
the next group in turn, and the larger group comes first. Of two accounts in
equally many bookings, the one money sits in leads, so a booking between an
account and a category reads as a flow out of, or into, the account:

```
2026-06-24 "Groceries"                  2026-06-24
Assets:Bank Expenses:Food 42.50 CHF       Groceries
                                        Assets:Bank
                                        -> Expenses:Food     42.50 CHF
```

A negative quantity is the same booking the other way round, so it is written
that way; only then does the arrow say where the money went. Nothing else
changes: the accounts, the amounts, the addon and the description are the
ones that were there, the description wrapped over as many lines as it needs.
A transaction whose description is empty has nothing to put below the date
and is left as it was.

### parse

```
fin parse journal.fin
```

Reads the journal and its includes and reports the first syntax or model
error, with its source location; silent and exit 0 if it is sound. The
balance assertions are verified when a report is built, so `fin balance` is
the stronger check.

## Development

```
cargo test
cargo fmt
```

### Golden tests

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

## License

Apache-2.0. See [LICENSE](LICENSE).
