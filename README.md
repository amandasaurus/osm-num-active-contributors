Parses an OSM History file, produces a spreadsheet (CSVs) of how many edit days
OSM accounts have.

As of May 2024, there are 10,000 OSM accounts which have 42+ edit days in the
last year.

# Usage

	osm-num-active-contributors -i history-latest.osm.pbf

# Output

If the `-p PREFIX` argument is given, theses files will start with `PREFIX`.

## `user_totals_per_day.csv`

CSV with 4 columns. One row per day in the range.

|Column name|Type|Description|
|-----------|----|-----------|
|`date`     |date (ISO format)|The date|
|`num_users`|Integer|Total number of users who have edited that day|
|`rolling_yr_total`|Integer|Total number of users who have edited from the date, to 1 year in the previously|
|`users_ge42_days`|Integer|Total number of users who, as of this date, have edited at 42 days or more in the last year|

## `users_per_day.csv`

One row per day per user in the range.

|Column name|Type|Description|
|-----------|----|-----------|
|`date`     |date (ISO format)|The date|
|`uid`|Integer|OSM User id|
|`num_edit_days_last_yr`|Integer|Total number of days this user has edited in the year ending on `date`|
|`username`|String|OSM username of this user, using the last seen username for this uid in the file|
|`ge42days`|Boolean (`yes`/`no`)|Has this user edited at least 42 days in the previous year of this date|
|`mapped_days`|String|Textual representation of all the mapping days for this user in the last year. Format is a `DD.MM.` separated by commas.|

# Cookbook

This will print the list of people who could get OSMF Active Contributor
membership on the date 2024-05-07.

	xsv search -s date 2024-05-07 ./users_per_day.csv | xsv search -s ge42days yes  | xsv select username,num_edit_days_last_yr | xsv sort -s num_edit_days_last_yr -N -R | xsv table

# Copyright

Code is released under the MIT/Apache-2 licence. See the `LICENCE-*` files.
