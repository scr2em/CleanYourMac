cask "cleanyourmac" do
  version "0.1.0-preview.2"
  sha256 "4e770431658c5aff3f3d318e516d9e34e7b0da874e094078fbd19dc378c56938"

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
