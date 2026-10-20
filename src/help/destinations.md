---
title: Destinations
kind: screen
screens: [destinations]
order: 7
summary: Where backups are kept: a folder or drive, an SMB share, a cloud folder or an S3 bucket.
---
A destination is where Keepr keeps backups. One destination can hold many plans, each in its own folder. Each card shows whether it's connected, how much Keepr keeps there and how much is free, and which plans it holds. **Change…** edits it. **Remove** asks first and only takes it off Keepr's list; nothing in it is deleted. It works once no plan uses it, or, for a destination that can't be found, removes the plans that use it too.

## Add a destination
**Add destination** in the toolbar, or [here](app:destinations/add). Pick the kind of place on the left and fill it in. **Test** writes a small file there, measures the speed and deletes it again. Then click **Add destination**.

## Folder or drive
A folder on this computer, or on a USB or Thunderbolt drive<!-- windows --> (on Windows, any drive but the system drive, by its letter)<!-- /windows -->. When a drive is unplugged its plans wait, and catch up when it's back. Keepr never writes to a drive's folder while the drive isn't there.

## SMB share
A share on a NAS or another computer. Keepr lists the servers it finds; click one or type its **Server**. Then the **Share** (**List** shows them), **User name**, **Password** and **Folder in the share**. Keepr connects when a backup needs it, <!-- mac -->out of sight of Finder<!-- /mac --><!-- github --> (on a Mac; on Windows <!-- /github --><!-- windows -->as `\\server\share`, with no drive letter<!-- /windows --><!-- github -->)<!-- /github -->, and disconnects after a few idle minutes.<!-- windows --> Only servers that announce themselves on the network are listed, as most NAS do; Windows PCs don't, so type their name or address. With no password entered, Windows uses the one it saved for that server (Remember my credentials).<!-- /windows -->

## Cloud folder on this computer
iCloud Drive, Google Drive, OneDrive, Dropbox, Box, and <!-- mac -->any service whose Mac app syncs a folder in Finder's sidebar under Locations<!-- /mac --><!-- github --> (Mac) or <!-- /github --><!-- windows -->any service that syncs a folder with Windows' cloud files (as OneDrive, iCloud for Windows, Dropbox and Box do), and Google Drive by its drive (G:, named Google Drive)<!-- /windows --><!-- github --> (Windows)<!-- /github -->. Keepr writes the backup into that folder and the service's app uploads it. The backup takes space on the computer too until the service offloads it: turn on its option for that, such as <!-- mac -->Optimise Mac Storage for iCloud<!-- /mac --><!-- github --> or <!-- /github --><!-- windows -->Files On-Demand for OneDrive<!-- /windows -->.

## S3 bucket
A bucket at Amazon S3, Backblaze B2, Cloudflare R2, or another S3 service (**Other**, with its endpoint and region). With a bucket and a key, fill them in. New to this? Each service has a helper:

- **Amazon S3**: **Let Keepr set it up** makes a private bucket and a key that can only use it, by signing in to AWS<!-- windows --> (on a Mac only)<!-- /windows --> or through AWS CloudShell.<!-- windows --> On Windows, use CloudShell.<!-- /windows -->
- **Backblaze B2**: enter a master key; Keepr uses it once to make the bucket and a key for it, and doesn't keep it. A bucket of that name that Keepr didn't make is left alone: choose another name.
- **Cloudflare R2**: enter a setup token; Keepr makes the bucket and a token for it, then deletes the setup token.

## Passwords and keys
Share passwords, bucket keys and encrypted plans' passwords and recovery keys are kept <!-- mac -->in one item in your login keychain, named Keepr<!-- /mac --><!-- github --> or <!-- /github --><!-- windows -->in Windows' Credential Manager, one for each, named Keepr/…, under Windows Credentials<!-- /windows -->. Nothing is written to a file.
