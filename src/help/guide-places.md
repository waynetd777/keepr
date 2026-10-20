---
title: Back up to a NAS or the cloud
kind: guide
order: 5
summary: Keep backups off this computer: on a NAS or another computer's share, in a cloud folder, or in an S3, B2 or R2 bucket.
---
## Pick the kind of place
A backup kept only on this computer goes with it. [Add a destination](app:destinations/add) and pick where the backups should go: **SMB share** for a NAS or another computer on your network, a cloud folder under **Cloud folders on this computer**, or **S3 bucket** for a cloud storage service.

## A NAS or another computer
In [Add a destination](app:destinations/add), choose **SMB share**. Click a server Keepr found, or type its name or address, then pick the **Share** (**List** shows them), and enter the user name and password. **Test** checks Keepr can write there. Keepr connects only when a backup needs it, and disconnects again afterwards.<!-- windows --> Windows PCs don't announce their shares: type the PC's name.<!-- /windows -->

## A cloud folder
OneDrive, iCloud Drive, Google Drive, Dropbox and the like appear under **Cloud folders on this computer** in [Add a destination](app:destinations/add). Keepr writes the backup into the folder, and the service's own app uploads it. The backup takes space on this computer too until the service offloads it, so turn on its option for that (<!-- mac -->Optimise Mac Storage, or Files On-Demand<!-- /mac --><!-- github --> or <!-- /github --><!-- windows -->Files On-Demand for OneDrive<!-- /windows -->).

## A bucket
Choose **S3 bucket** in [Add a destination](app:destinations/add) and the service: Amazon S3, Backblaze B2, Cloudflare R2 or another. If you have a bucket and a key, fill them in. If not, each service has a helper that makes a private bucket and a key that can only use it: **Let Keepr set it up** for Amazon, a master key for B2, a setup token for R2.

## Use it in a plan
A plan's **Where** picks its destination. [Make a plan](app:plans/new) for the new place, or keep two copies by making a second plan with the same folders and another destination. [Destinations](app:destinations) shows how much each place holds and has free.
