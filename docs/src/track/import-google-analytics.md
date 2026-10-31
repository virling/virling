# Import from Google Analytics

Bring your Google Analytics 4 history into Virling, so dashboard ranges before you added the Virling script still show your traffic. There is no Google sign-in: you export CSV files and import the folder.

## Export from GA4

GA4's standard reports download totals for the whole range, so use an exploration, which keeps one row per day:

1. In Google Analytics, open **Explore** and start a **Free form** exploration over the range you want.
2. Add the dimension **Date** and the metrics **Active users**, **Sessions** and **Views**. Put **Date** in Rows and the metrics in Values.
3. Export it as CSV (**Download file**, **CSV**). This file is the daily total.
4. For each breakdown you want, add one more dimension to Rows next to **Date** and export again: **Session source / medium**, **Page path and screen class**, **Country** and **Device category**. Use one breakdown per file.

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
| Date with users, views and sessions | Visitors, visits and the daily chart |
| Session source / medium | Channels and referrers |
| Page path | Pages |
| Country | Countries |
| Device category | Devices |

The rules are the same as for the [Plausible import](import-plausible.md): only days before the first event Virling tracked are kept, imported visitors are summed day by day, filters show tracked visits only, and payments are only ever credited to tracked visits.
