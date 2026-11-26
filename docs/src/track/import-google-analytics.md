# Import from Google Analytics

Bring your Google Analytics 4 history into Virling, so dashboard ranges before you added the Virling script still show your traffic. There is no Google sign-in: you export CSV files and import the folder.

## Export from GA4

GA4's standard reports download totals for the whole range, so use an exploration, which keeps one row per day. Google's guides: [Explorations](https://support.google.com/analytics/answer/7579450) and [Free form](https://support.google.com/analytics/answer/9327972).

1. In Google Analytics, click **Explore** in the left navigation and pick the **Free form** template. Set the date range in the top left.
2. Under **Variables**, click the plus next to **Dimensions** and import **Date**, then next to **Metrics** and import **Active users**, **Sessions** and **Views**. Drag **Date** to **Rows** and the metrics to **Values**.
3. In the upper right, click **Export data** and choose **CSV (comma separated values)**. This file is the daily total.
4. For each breakdown you want, add one more dimension to **Rows** next to **Date** and export again: **Session source / medium**, **Page path and screen class**, **Country** and **Device category**. Use one breakdown per file.

Put all the files in one folder. Each file needs a Date column and at most one of the breakdowns above. GA4's comment lines at the top are skipped, and when a download holds several tables only the first is read. A file with any other breakdown, such as City or Session default channel group, is skipped and the import says which and why, so it is never counted as a total.

### From BigQuery

If GA4 exports to BigQuery, a query can make the same files. This one gives the daily total; add `collected_traffic_source.manual_source AS source, collected_traffic_source.manual_medium AS medium`, `geo.country AS country` or `device.category AS device` to the select and the group by for a breakdown, and save each result as CSV:

```sql
SELECT
  PARSE_DATE('%Y%m%d', event_date) AS date,
  COUNT(DISTINCT user_pseudo_id) AS users,
  COUNTIF(event_name = 'page_view') AS pageviews,
  COUNT(DISTINCT CONCAT(user_pseudo_id, CAST(
    (SELECT value.int_value FROM UNNEST(event_params) WHERE key = 'ga_session_id') AS STRING))) AS sessions
FROM `your-project.analytics_123456.events_*`
GROUP BY date
```

## Import

Add the site to Virling first, then stop the server and run:

```sh
virling import-ga example.com ./ga4-export
```

Then start the server again. Running an import again replaces the earlier one, from Google Analytics or Plausible.

## What is imported

| Column | Shows in |
| --- | --- |
| Date with Active users and Sessions | Visitors, visits and the daily chart |
| Session source / medium | Channels and referrers |
| Page path and screen class | Pages |
| Country | Countries |
| Device category | Devices |

Each row adds its **Active users** as visitors, and the total file's **Sessions** as visits. Channels are worked out from the source and medium with Virling's own rules, so they can differ from GA4's channel groups. **Views** are read and kept, but no report shows them yet.

These come across under other names too: **Session source**, **First user source** or **Source** with a **Medium** column, **Page path** or **Landing page**, **Total users** or **Users**, and dates written as `20240101` or `2024-01-01`.

Nothing else does: no payments or revenue, key events or conversions, goals, cities or regions, browsers, operating systems, campaigns, or single visits. Payments come from your payment provider instead, and only for visits Virling tracked.

## How imported days show

- Only days before the first event Virling tracked for the site are kept, so a day is never counted twice.
- Imported visitors and visits are added to the totals, to the daily chart and to the previous period you compare against. The hourly chart of a single day shows tracked visits only.
- In a breakdown, an imported row adds visitors to the matching row, or shows on its own with no revenue.
- Imported visitors are summed day by day, so a person who visited on two days counts twice over a longer range.
- **Filters show tracked visits only.** A filter on a channel, page or country leaves imported traffic out, since the export has no per-visit detail.
- **Payments are only credited to tracked visits.** Revenue per visitor and conversion use tracked visitors only, and visitor lists, journeys, goals and funnels leave imported days out.

The rules are the same as for the [Plausible import](import-plausible.md).

## Fix a failed import

The import prints one line for each file it skips.

- **`skipped pages.csv: it has a column Virling does not import: City`.** The file has a breakdown Virling does not read, or two breakdowns at once. Export it again with **Date** and one of the breakdowns above in **Rows**, or delete the file.
- **`skipped report.csv: it has no Date column`**, or **`no Google Analytics CSV export with a Date column`**. The file came from a standard report, or **Date** was not in **Rows**. Export from a Free form exploration with **Date** in **Rows**. Files must end in `.csv`, in lowercase.
- **`Imported 0 daily rows for example.com.`** Every day in the files is on or after the first day Virling tracked, so none were kept. Export a range that ends the day before you added the Virling script.
