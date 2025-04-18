# Parti-VOD-Downloader



## macOS Installation

### 1. Move the App to Applications

- After downloading, **drag** `Parti VOD Downloader.app` into your **Applications** folder.

---

### 2. Open the App for the First Time

When you open the app for the first time, macOS may warn you that it is from an unidentified developer.

#### **Standard Opening Process**

1. Open your **Applications** folder.
2. **Double-click** on `Parti VOD Downloader.app` to open it.
3. You may see a message like:  
   > "Parti VOD Downloader.app can't be opened because it is from an unidentified developer."
4. Click **Cancel** to close the warning.

---

#### **Allow the App in System Settings (Privacy & Security)**

If you see the warning above, follow these steps:

1. Open **System Settings** (or **System Preferences** on older macOS).
2. Go to **Privacy & Security**.
3. Scroll down to the **Security** section.
4. You should see a message:  
   > "Parti VOD Downloader.app was blocked from use because it is not from an identified developer."
5. Click the **Open Anyway** button next to this message.
6. A new dialog will appear. Click **Open** to confirm.

> **Note:**  
> You only need to do this the first time. After that, you can open the app normally.

---

### 3. If You See a "Damaged" Error

If you see:

> "Parti VOD Downloader.app is damaged and can’t be opened. You should move it to the Trash."

This is a security feature. To fix it:

#### **Remove the Security Flag (Advanced Users)**

1. Open the **Terminal** app (in Applications > Utilities).
2. Copy and paste this command, then press **Enter**:

   ```sh
   xattr -dr com.apple.quarantine /Applications/Parti\ VOD\ Downloader.app
   ```

3. Try opening the app again using the steps above.

> **What does this do?**  
> This command removes a security flag that can block the app from opening. It is safe to use for apps you trust.

---

### 4. After the First Launch

- After you have opened the app once, you can open it normally from the Applications folder, Launchpad, or Spotlight.

---