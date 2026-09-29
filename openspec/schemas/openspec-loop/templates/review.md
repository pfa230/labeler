<!-- DO NOT FILL THIS IN BY HAND. A review record is produced by:

       openspec-loop record propose --change <name>
       openspec-loop record implement --change <name>

     which computes FORK_POINT, SUBJECT_SHA256 and the MANIFEST block from the repository.
     A record written from this file carries none of them and every check that reads a
     manifest refuses it. The form below is here so you can recognise a well-formed record,
     not so you can copy it. -->

REVIEWER: <reviewer>
PRODUCERS: <producers>
VERDICT: <APPROVE, APPROVE_WITH_CHANGES or REVISE>
FORK_POINT: <the merge base this record is bound to, written by the command>
SUBJECT_SHA256: <the digest of the MANIFEST block, written by the command>
MANIFEST:
<one entry per line, written by the command>

<!-- On a propose APPROVE_WITH_CHANGES only, added by the author, never by the command: -->

CHANGES_APPLIED: yes
REQUIRED_CHANGES:
- <one required edit per line>

<!-- An `implement` record also carries CHANGE_PATH, written by the command.
     Lines that are neither a field nor part of a block field are prose and are ignored,
     so the reviewer's findings may follow. -->
