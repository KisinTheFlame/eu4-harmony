# Keep current guidance in one place and retain useful evidence

Documentation should make current conclusions easy to find and avoid repeated maintenance. README introduces the project and its first steps; `eu4-harmony wiki` describes capabilities; skills explain procedures; game pages describe game systems; research holds their evidence; code comments explain implementation. Other documents summarize and link to the primary account. Historical plans are not instructions for current use, and the glossary holds definitions only.

Completed plans and repeated test records may be consolidated or removed after their useful conclusions, unique research and address knowledge, key evidence, failure reasons and unverified behavior have been retained and references repaired. Keeping every original file would preserve the reading and maintenance burden; moving everything into an archive would not resolve it.

Documentation rules must support this division. A coherent validation may summarize several runs, including significant failures and limits, rather than require a file for each run. README changes when the introduction, installation or first steps change, rather than for every visible change. Navigation points to useful entry pages instead of listing every document.

Ordinary builds, unit tests and routine regressions belong in the PR validation description, not permanent run files or screenshots. Repository evidence is reserved for findings with lasting value that code and repeatable tests do not already preserve. Existing logs are consolidated by topic; keep a screenshot only when the image itself supplies irreplaceable evidence.
