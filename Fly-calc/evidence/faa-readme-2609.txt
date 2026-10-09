Aeronautical Information Services     8/12/2026 1 of 7


U.S. Department of Transportation
Federal Aviation Administration
Aeronautical Information Services
http://www.faa.gov/go/ais

FAA/Aeronautical Information Services CIFP Readme

Volume:  2609

Effective:  0901Z
03 September 2026
To:  0901Z
01 October 2026

Last Transmittal Letter:  31 July 2026



Customer Agreement Form
Please note: When downloading the CIFP, you have agreed to the Customer Agreement for Error
Notification by clicking “I Agree” on the FAA Website.

As a recipient of the CIFP, you agree to the following:

⋅ You must notify all your customers who receive or download the CIFP, or CIFP-derived data
product, of the subject and nature of the reported error.
⋅ You must notify the FAA contact listed below if you discover an error in the CIFP, or if one
of your customers notifies you of an error.
⋅ Upon notification and verification of any CIFP error that is discovered and reported by you or
any of your customers, the FAA will in turn notify all CIFP customers of the error.

The FAA contact is:

Federal Aviation Administration
Aeronautical Information Services, Room 507-19
800 Independence Avenue, SW
Washington, DC  20591
https://www.faa.gov/air_traffic/flight_info/aeronav/aero_data/Aeronautical_Inquiries/

By subscribing to http://www.faa.gov/air_traffic/flight_info/aeronav/safety_alerts/ you will be
notified by email of any errors discovered and reported to the FAA.


Aeronautical Information Services     8/12/2026 2 of 7
ARINC 424 Standard

The CIFP adheres to ARINC 424-18 specifications, with exceptions noted forthwith.


Record types

The Coded Instrument Flight Procedures (CIFP) currently provides the following record types:
• Airports and heliports (PA and HA)
• Runways (PG)
• VHF Navaids (D)
• NDB Navaids (DB)
• Terminal Navaids (PN)
• Localizer and Glide Slope Records (PI)
• Path Point Records, Primary and Continuation (PP)
• MSA Records (PS and HS)
• Enroute Waypoints (EA)
• Terminal Waypoints (PC and HC)
• SIDs (PD)
• STARs (PE)
• Approaches, including Level of Service continuation records (PF and HF)
• Airways (ER)
• Class B, C, and D Airspace (UC)
• Special Use Airspace, Primary and Continuation (UR)
• Grid MORA (AS)


Airports and Heliports:  The published ICAO Airport Identifier will be used (5.6). If there is no
published ICAO Airport Identifier, then the published FAA Airport Identifier will be used.  The
IATA code field in the Airport Record will contain the FAA Airport Identifier.  If the Airport
Identifier is four characters in length, the field will be left blank.

The Longest Runway (5.54) field may not always represent the longest hard-surface runway at the
airport.

Runways:  Suffixes for water runways (W), soft-surface runways (S), glider runways (G), ultralight
runways (U) and assault strips (numeric) will be included (5.46).

Non-numeric runway identifiers (5.46) are included and will not carry the prefixed ‘RW’ characters.

Runway gradient (5.212) and ellipsoid height (5.225) are included in the runway record when
available.

If a magnetic variation is not available to help determine Runway Magnetic Bearings (5.58), one will
be calculated using the WMM calculator.

Aeronautical Information Services     8/12/2026 3 of 7
Waypoints and Fixes: Domestic fixes include the following types:  Reporting point, Waypoint,
RNAV Waypoint, VFR Waypoint, Computer Navigation, Terminal Waypoint, ATC Coordination,
GPS Waypoint, Military Reporting Point, Military Waypoint, NRS Waypoint, and Radar.  Waypoints
with all-numeric identifiers will not be included.

Fixes classified as “Offshore” in the NASR database may be assigned a Customer/Area Code (5.3) of
‘USA’ and an ICAO Code (5.14) of “K “ or “P “ (Character “K” or “P” followed by blank or NULL
value).

Waypoint Type (5.42, col 27) designates fixes with an “R” for ground-based components, a “W” for
satellite-based components, and a “C” for both.  Fixes that can only be defined with RADAR will be
designated with an “R.”

Waypoint Name Format Indicator (5.196) will not be populated.

PC records (4.1.4.1) may be used for named terminal waypoints if used at only one airport in the
CIFP, and not used on an Enroute Airway.  Otherwise, EA records will be used.  In some cases, PC
records also are used for unnamed terminal waypoints.

PC Waypoints will carry the Customer/Area Code (5.3) of the parent airport, regardless of its
location.  PC waypoints, however, will maintain their distinct ICAO Code (5.14), even if it differs
from that of the parent airport.

Navaids:  A PN Record may be used for NDBs when only used at one airport in the CIFP, when not
used on an Enroute Airway, and when assigned a five-letter name.  Otherwise, DB records will be
used.

DME only facilities are not assigned station declination values in the NASR NAV.txt subscriber file.
A magnetic variation is computed using the WMM calculator to support ERAM requirements.

If a VOR frequency is unavailable, the VOR Frequency field (5.34) will contain ‘00000’ in columns
23-27.  For all other Navaids, an unavailable frequency will result in blank coding.

Navaid Class 3 (5.35) will be coded as ‘H’ for high, ‘L’ for low, and ‘T’ for terminal altitude
description.  Where undetermined, the field will be coded with ‘U’.  Navaid Class 5 will carry an ‘N’
for VORTACs if the VOR coordinates and the TACAN coordinates are 0.1 NM or greater distance
from each other.

DME elevation (5.40) is not available in NASR NAV.txt subscriber file when it differs from an
associated VOR or TACAN facility.  In these instances, the VOR elevation will be populated.

The Figure of Merit (5.149) is determined from the NAVAID Class. If the Navaid Class is
undetermined, the Figure of Merit will be coded as ‘3’

Weather Capability codes for Hazardous Inflight Weather Advisory Service (HIWAS) and Automatic
Transcribed Weather Broadcast (TWEB), will be coded with an “A” (5.35).

Aeronautical Information Services     8/12/2026 4 of 7
Airways:  U.S. Airways include Enroute Airways, Area Navigation Routes, Area Navigation
Instrument Flight Rules Terminal Transition Routes, and ATS Routes.  Non-US airways, including
Canadian Airways, will no longer be included in the dataset.

Route Type (5.7) will employ “O” for conventional and “R” for RNAV routes.  The point-to-point
MEA value in the Minimum Altitude (5.30) field will be coded.  If the airway is a conventional route,
the conventional MEA will be coded.  If the airway is an RNAV route, the GNSS MEA will be
coded.  If a GNSS MEA value is not provided, the conventional MEA will be used.

Airways and ATS Routes will contain a Level entry (5.19) if the route begins with V or T (Level = L)
and J or Q (level =H).  Otherwise, the Level will be blank.


ATS Routes will not contain directional restrictions (5.115).

Approaches: Standard instrument approach procedures include ILS and/or LOC (Category 1 only),
VOR and NDB-related procedures including GPS overlays, GPS, RNAV (GPS), RNAV (RNP), as
well as GPS and RNAV (GPS) helicopter approaches.  ILS component records (PI records) will only
be added for those procedures that are included in the CIFP.

For LDA approaches with both LDA and Glide Slope minima, the procedure will be coded to LDA
minimums only.

Version 19 of the ARINC 424 specifications will be applied with regard to SID/STAR/Approach
Identifier (5.9, 5-10, col 14-19), Route Type (5.7, col 20), and Route Qualifier 1 (5.7, col 119). For
RNAV (RNP) approaches, Column 20 will be populated with an H, and column 119 will be
populated with an F.  In addition, the route identifier for the final and missed approach segments of
these procedures will be identified with an H (5.10, col 14).  RF legs will be coded as fly by except
when followed by a Hx leg type.

Version 19 of the ARINC 424 specifications will be applied with regard to the level of the service
continuation record (5.297)

Version 19 of the ARINC 424 specifications, Attachment 5, para 6.10.3.2 will be applied with regard
to the altitude 1 field for circling procedures that are not straight-in aligned with a runway.

Waypoint description code H (5.17) will not be used for arrival, SID, or STAR holding.  Holding
records (EP) are not available in the database to reference.

Controlled Airspace: Controlled Airspace Names (5.216) are those found in the legal description.

Airspace Center (5.214) uses the ICAO identifier for the primary airport for the airspace.

Unit Indicator (5.133) for all altitudes described as “GND” contain an A for AGL.

Special Use Airspace: Time Code (5.131) uses a “C” to indicate continuous and is blank to
indicate part-time.
Aeronautical Information Services     8/12/2026 5 of 7
Special Use Airspace Altitudes (5.121) are only coded on the first record of each
area. Upper altitudes are described as "to and including".  The Unit Indicator (5.133)
for all altitudes described as “GND” contains an A for AGL.

Positions within UR records that result in a gap (greater than 0.02NM) between an arc end and its
adjoining leg may have been recalculated. The revised coordinate positions more accurately reflect
the arc radius published in the legal description but may differ from the published coordinates.
The lower and upper limits (5.121) of a totally excluded volume from within an airspace will be
coded as GND  A0000A.

Continuation Records for UR records are included only for Controlling Agencies (5.140).

Special Air Traffic Rules Areas described under Part 93 of Title 14 of the CFR are included as UR
Records if their spatial dimensions can be coded. These are classified as “U” in the Restricted
Airspace Type (5.128). Please refer to Part 93 of the CFR for the specific air traffic rules governing
these areas.
Restrictive Airspace Designation (5.129) listed as follows include published names in parentheses.
ANC SATR (Anchorage, Alaska, Terminal Area)
KTN SATR (Ketchikan International Airport Traffic Rule)
GCNPSFRA E and GCNPSFRA W (Grand Canyon Special Flight Rules) See below for details
LUKE SATR (Special Air Traffic Rules in vicinity of Luke AFB, AZ)
DC SFRA (Washington, DC Metropolitan Area Special Flight Rules Area) includes DC FRZ
NIAGFLSATR (Special Air Traffic Rules in vicinity of Niagara Falls, New York)
NY SFRA (New York Class B Airspace Hudson River and East River Exclusion SFRA)
VALPO SATR (Valparaiso, Florida Terminal Area)
VUO SFRA (Special Air Traffic Rules in vicinity of Portland Intl, OR)

National Security Areas are included as UR records with a “U” coded in the Restrictive Airspace
Type field (5.128)
The GRAND CANYON SFRA is divided into East and West sections. The internal boundaries
for Sectors and Flight Free Zones (FFZ) are included.  All boundary names and their altitudes are
included in the Restrictive Airspace Name Field (5.216).  The Restrictive Airspace Designation
(5.129) is "GCNPSFRA E" or "GCNPSFRA W".  Please refer to the Part 93 Subpart U of the CFR
for the specific air traffic rules governing the Grand Canyon SFRA.

UR airspace involving nautical/maritime limits are in the process of being standardized to the official
NOAA source, while maintaining the coordinates given in the airspace legal description. This
sometimes results in the given points not meeting the nautical boundary.

Coding information

Waypoint Descriptor:  Compulsory waypoints will not be indicated in Waypoint Descriptor 3 (5.17)
for all routes.  An “R” is coded in Waypoint Descriptor 3 (5.17) when a fix marks a course change in
the final approach.  The ‘S’ attribute in Waypoint Descriptor 3 (5.17) will not be coded for step down
fixes between the FACF and the FAF.  For RNAV (RNP) procedures, the ‘S” attribute will not be
coded for step down fixes between the FAF and the MAP.
Aeronautical Information Services     8/12/2026 6 of 7
Final Approach Course Fix (FACF): The FACF may be designated as the Initial Fix (IF).  This
may result in Step-down Fixes between the FACF and the FAF.

Minimum Safe Altitude (MSA):  The MSA Center Fix (5.144) is normally coded on the FAF
record.  If the FAF record is an RF leg, then the MSA Center Fix is coded on the FACF record.  If
there is no FACF record, a center fix will not be coded.

Vertical Angle: Circling and Dive-and-Drive procedures are coded with “000” in the vertical angle
field (5.70).  Additionally, “000” will be coded for straight in aligned procedures as directed by Flight
Inspection when obstacles are identified in the visual areas.

Missed Approach:  Course-to-Altitude (CA) path terminator segments may be used as the first leg of
the missed approach.  If there is no mandatory altitude specified in the missed approach instructions,
the CIFP will code the lowest of the DA, the MDA, or 400 feet above airport elevation.

Alternate missed approaches (Route Type Z) (5.7) are not included in the CIFP.  Route type for the
missed approach will reflect the route type of the final approach.

Required Navigation Performance (RNP):  For standardization of CIFP RNP Value coding (para
5.211), NAVSPEC values found in FAAO 8260.58C, Table 1-2-1 will be populated using the
following methods:

1. Day-forward procedures, HF and HM legs will not be coded with RNP Values in the CIFP.
2. For amendments and abbreviated amendments, RNP Values will match those published in FAA
Form 8260-3 Terminal Routes section.
3. For P-NOTAMs that require coding changes, RNP Values will match FAA Form 8260-3 Terminal
Routes section.  If no RNP Values are documented, RNP Values will be coded to reflect current
NAVSPEC without regard to RNP Values that may be on an accompanying FAA Form 8260-10.
4. For P-NOTAMs that do not require coding changes, coding will be updated on a time permitting
basis and RNP Values will be coded using the standard in #3.
5. Day-back procedures already in the CIFP will be updated as workload permits. If one or more
procedures at an airport are updated to current NAVSPEC, an effort will be made to update other
procedures at the airport at the same time.

Route Qualifier 2:  Route Qualifier 2  (5.7) of “H” will be shown for copter approaches using
helipads.

File Record Number (5.31) A unique number is assigned for each record rather than consecutively
for the entire dataset.  Some file record numbers will have alphabetic characters or blank fields.

Cycle Date (5.32) Cycle dates will be updated to the most recent cycle on all new records in the
CIFP, or on those records with modifications.

General information:

Header Records (6.2.1) It has been identified that character 62 is a blank space and not the actual
starting letter of the Data Supplier Ident.  This will be resolved at a later date.

Aeronautical Information Services     8/12/2026 7 of 7
Procedure types that are not yet included in the CIFP: ILS CAT II, ILS CAT III, PRM,
Converging ILS, GLS, and Visual procedures are not included in the CIFP.

Not-In-CIFP Spreadsheet:  ILS (Category 1 only), LOC, SDF, LDA, VOR, NDB, GPS, RNAV
(GPS), RNAV (RNP), GPS Overlays, GPS and RNAV (GPS) helicopter approaches, GLS, SIDs and
STARs that do not appear in the CIFP will be listed in the Not-In-CIFP spreadsheet.
Only FAA-approved public use procedures are included in the CIFP.  Approach types listed above
that are developed outside the FAA, such as Navy or Air Force procedures, will not appear in this
spreadsheet.

Magnetic Variation:  Calculations are derived from the
World Magnetic Model, 2025 epoch year.  For
more information on the WMM, and access to the 2025-2029 coefficients go to:
https://www.ngdc.noaa.gov/geomag/WMM/DoDWMM.shtml

To align CIFP dynamic magnetic variation calculations with NASR subscriber AWY.txt and ATS.txt
files, if there is no magnetic variation assigned by official government source, the CIFP dynamic
magnetic variation will be calculated based on the magnetic epoch of the first 56-day cycle following
the first 56-day cycle whose data processing falls fully within the calendar year.  For 2026, this
occurs in CIFP 2605 (effective 14 May 2026).

Dynamic magnetic variation values for Waypoint Records and DME Only facilities will be calculated
using the magnetic epoch in CIFP 2605 (effective 14 May 2026).

Canadian data:  Refer to current Canadian charts and flight information publications for information
within Canadian airspace.

Non-FAA procedure coding will be included if submitted to the FAA by the entity responsible for
procedure development.  The format may not adhere to FAA coding practices.

Cyclic Redundancy Check (CRC), Data Replication Integrity

The Coded Instrument Flight Procedures is wrapped with a 32-bit CRC, calculated as described in
ARINC Report 665.

Comments

Aeronautical Information Services welcomes any comments, suggestions and inquiries regarding
specific coding practices that could improve the Coded Instrument Flight Procedures.

Please contact Aeronautical Information Services at:

Federal Aviation Administration
Aeronautical Information Services, Room 507-19
800 Independence Avenue, SW
Washington, DC  20591
https://www.faa.gov/air_traffic/flight_info/aeronav/aero_data/Aeronautical_Inquiries/
