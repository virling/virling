# Import from Plausible

Bring your Plausible history into Virling, so dashboard ranges before you added the Virling script still show your traffic.

## Export from Plausible

1. In Plausible, open the site's settings. Under **Imports & Exports**, in **Export Data**, click **Export to CSV**. Plausible's guide: [export stats](https://plausible.io/docs/export-stats).
2. When the email arrives, download the zip and unzip it. The folder holds files such as `imported_visitors_20240101_20241231.csv`.

## Import

Add the site to Virling first, then stop the server and run:

```sh
virling import-plausible example.com ./plausible-export
```

Then start the server again. Running the import again replaces the earlier import.

## What is imported

| File | Shows in |
| --- | --- |
| `imported_visitors_*` | Visitors, visits and the daily chart |
| `imported_sources_*` | Channels and referrers |
| `imported_pages_*` | Pages |
| `imported_locations_*` | Countries |
| `imported_devices_*` | Devices |

Other files in the export are ignored. Imported visitors are summed day by day, so a person who visited on two days counts twice over a longer range.

Only days before the first event Virling tracked for the site are kept, so a day is never counted twice.

## What it does not change

Imported traffic is stored apart from tracked visits:

- **Payments are only credited to tracked visits.** Imported days have no visitors to match, so attribution, revenue per visitor and conversion use tracked visits only.
- **Filters show tracked visits only.** A filter on a channel, page or country leaves imported traffic out, since the export has no per-visit detail.
- **Visitor lists, journeys, goals and funnels** use tracked visits only.

Uploading an export in site settings is <span class="na">Not available yet</span>.
