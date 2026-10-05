---
title: Destinations
kind: screen
screens: [destinations]
order: 7
summary: Where backups are kept: a folder or drive, an SMB share, a cloud folder or an S3 bucket.
---
A destination is where Keepr keeps backups. One destination can hold many plans, each in its own folder. Each card shows whether it's connected, how much Keepr keeps there and how much is free, and which plans it holds. **Change…** edits it; **Remove** works once no plan uses it.

## Add a destination
**Add destination** in the toolbar, or [here](app:destinations/add). Pick the kind of place on the left and fill it in. **Test** writes a small file there, measures the speed and deletes it again. Then click **Add destination**.

## Folder or drive
A folder on this Mac, or on a USB or Thunderbolt drive. When a drive is unplugged its plans wait, and catch up when it's back. Keepr never writes to a drive's folder while the drive isn't there.

## SMB share
A share on a NAS or another computer. Keepr lists the servers it finds; click one or type its **Server**. Then the **Share** (**List** shows them), **User name**, **Password** and **Folder in the share**. Keepr connects when a backup needs it, out of sight of Finder, and disconnects after a few idle minutes.

## Cloud folder on this Mac
iCloud Drive, Google Drive, OneDrive, Dropbox, Box, and any service whose Mac app syncs a folder in Finder's sidebar under Locations. Keepr writes the backup into that folder and the service's app uploads it. The backup takes space on the Mac too until the service offloads it: turn on its option for that, such as Optimise Mac Storage for iCloud.

## S3 bucket
A bucket at Amazon S3, Backblaze B2, Cloudflare R2, or another S3 service (**Other**, with its endpoint and region). With a bucket and a key, fill them in. New to this? Each service has a helper:

- **Amazon S3**: **Let Keepr set it up** makes a private bucket and a key that can only use it, by signing in to AWS or through AWS CloudShell.
- **Backblaze B2**: enter a master key; Keepr uses it once to make the bucket and a key for it, and doesn't keep it.
- **Cloudflare R2**: enter a setup token; Keepr makes the bucket and a token for it, then deletes the setup token.

## Passwords and keys
Share passwords, bucket keys and encrypted plans' passwords and recovery keys are kept in one item in your login keychain, named Keepr. Nothing is written to a file.
