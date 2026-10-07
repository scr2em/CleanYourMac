cask "cleanyourmac" do
  version "0.1.0-preview.1"
  sha256 "1227ce0bc14f34c257ddd4fcebe5f1412140465d12f57db6403ec10d7a555649"

  url "https://github.com/scr2em/CleanYourMac/releases/download/v#{version}/CleanYourMac-#{version}-universal.zip"
  name "CleanYourMac"
  desc "Find and review storage clutter, project dependencies, and orphan processes"
  homepage "https://github.com/scr2em/CleanYourMac"

  depends_on macos: ">= :sonoma"

  app "CleanYourMac.app"

  caveats <<~EOS
    This is an early preview. The app is not Apple notarized yet.
    Scans are read-only; cleanup requires selecting items and reviewing an action.
  EOS
end
