#!/usr/bin/env node
// Collect published license/copyright notices without executing package code.
// Run after pnpm install and cargo fetch, before the frontend/native build.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const destination = path.join(root, 'public/licenses/DEPENDENCIES.txt');
const warnings = [];
// Canonical license terms from the pinned SPDX License List release. These
// supplement packages whose published archives omit a standalone license.
// They never substitute invented copyright owners or years for upstream data.
const standardLicenses = {"MIT":{"source":"https://raw.githubusercontent.com/spdx/license-list-data/v3.27.0/json/details/MIT.json","text":"MIT License\n\nCopyright (c) <year> <copyright holders>\n\nPermission is hereby granted, free of charge, to any person obtaining a copy of this software and\nassociated documentation files (the \"Software\"), to deal in the Software without restriction, including\nwithout limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell\ncopies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the\nfollowing conditions:\n\nThe above copyright notice and this permission notice shall be included in all copies or substantial\nportions of the Software.\n\nTHE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT\nLIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO\nEVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER\nIN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE\nUSE OR OTHER DEALINGS IN THE SOFTWARE.\n"},"BSD-3-Clause":{"source":"https://raw.githubusercontent.com/spdx/license-list-data/v3.27.0/json/details/BSD-3-Clause.json","text":"Copyright (c) <year> <owner>. \n\nRedistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:\n\n1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.\n\n2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.\n\n3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.\n\nTHIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS \"AS IS\" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.\n"},"MPL-2.0":{"source":"https://raw.githubusercontent.com/spdx/license-list-data/v3.27.0/json/details/MPL-2.0.json","text":"Mozilla Public License Version 2.0\n==================================\n\n1. Definitions\n--------------\n\n1.1. \"Contributor\"\n    means each individual or legal entity that creates, contributes to\n    the creation of, or owns Covered Software.\n\n1.2. \"Contributor Version\"\n    means the combination of the Contributions of others (if any) used\n    by a Contributor and that particular Contributor's Contribution.\n\n1.3. \"Contribution\"\n    means Covered Software of a particular Contributor.\n\n1.4. \"Covered Software\"\n    means Source Code Form to which the initial Contributor has attached\n    the notice in Exhibit A, the Executable Form of such Source Code\n    Form, and Modifications of such Source Code Form, in each case\n    including portions thereof.\n\n1.5. \"Incompatible With Secondary Licenses\"\n    means\n\n    (a) that the initial Contributor has attached the notice described\n        in Exhibit B to the Covered Software; or\n\n    (b) that the Covered Software was made available under the terms of\n        version 1.1 or earlier of the License, but not also under the\n        terms of a Secondary License.\n\n1.6. \"Executable Form\"\n    means any form of the work other than Source Code Form.\n\n1.7. \"Larger Work\"\n    means a work that combines Covered Software with other material, in \n    a separate file or files, that is not Covered Software.\n\n1.8. \"License\"\n    means this document.\n\n1.9. \"Licensable\"\n    means having the right to grant, to the maximum extent possible,\n    whether at the time of the initial grant or subsequently, any and\n    all of the rights conveyed by this License.\n\n1.10. \"Modifications\"\n    means any of the following:\n\n    (a) any file in Source Code Form that results from an addition to,\n        deletion from, or modification of the contents of Covered\n        Software; or\n\n    (b) any new file in Source Code Form that contains any Covered\n        Software.\n\n1.11. \"Patent Claims\" of a Contributor\n    means any patent claim(s), including without limitation, method,\n    process, and apparatus claims, in any patent Licensable by such\n    Contributor that would be infringed, but for the grant of the\n    License, by the making, using, selling, offering for sale, having\n    made, import, or transfer of either its Contributions or its\n    Contributor Version.\n\n1.12. \"Secondary License\"\n    means either the GNU General Public License, Version 2.0, the GNU\n    Lesser General Public License, Version 2.1, the GNU Affero General\n    Public License, Version 3.0, or any later versions of those\n    licenses.\n\n1.13. \"Source Code Form\"\n    means the form of the work preferred for making modifications.\n\n1.14. \"You\" (or \"Your\")\n    means an individual or a legal entity exercising rights under this\n    License. For legal entities, \"You\" includes any entity that\n    controls, is controlled by, or is under common control with You. For\n    purposes of this definition, \"control\" means (a) the power, direct\n    or indirect, to cause the direction or management of such entity,\n    whether by contract or otherwise, or (b) ownership of more than\n    fifty percent (50%) of the outstanding shares or beneficial\n    ownership of such entity.\n\n2. License Grants and Conditions\n--------------------------------\n\n2.1. Grants\n\nEach Contributor hereby grants You a world-wide, royalty-free,\nnon-exclusive license:\n\n(a) under intellectual property rights (other than patent or trademark)\n    Licensable by such Contributor to use, reproduce, make available,\n    modify, display, perform, distribute, and otherwise exploit its\n    Contributions, either on an unmodified basis, with Modifications, or\n    as part of a Larger Work; and\n\n(b) under Patent Claims of such Contributor to make, use, sell, offer\n    for sale, have made, import, and otherwise transfer either its\n    Contributions or its Contributor Version.\n\n2.2. Effective Date\n\nThe licenses granted in Section 2.1 with respect to any Contribution\nbecome effective for each Contribution on the date the Contributor first\ndistributes such Contribution.\n\n2.3. Limitations on Grant Scope\n\nThe licenses granted in this Section 2 are the only rights granted under\nthis License. No additional rights or licenses will be implied from the\ndistribution or licensing of Covered Software under this License.\nNotwithstanding Section 2.1(b) above, no patent license is granted by a\nContributor:\n\n(a) for any code that a Contributor has removed from Covered Software;\n    or\n\n(b) for infringements caused by: (i) Your and any other third party's\n    modifications of Covered Software, or (ii) the combination of its\n    Contributions with other software (except as part of its Contributor\n    Version); or\n\n(c) under Patent Claims infringed by Covered Software in the absence of\n    its Contributions.\n\nThis License does not grant any rights in the trademarks, service marks,\nor logos of any Contributor (except as may be necessary to comply with\nthe notice requirements in Section 3.4).\n\n2.4. Subsequent Licenses\n\nNo Contributor makes additional grants as a result of Your choice to\ndistribute the Covered Software under a subsequent version of this\nLicense (see Section 10.2) or under the terms of a Secondary License (if\npermitted under the terms of Section 3.3).\n\n2.5. Representation\n\nEach Contributor represents that the Contributor believes its\nContributions are its original creation(s) or it has sufficient rights\nto grant the rights to its Contributions conveyed by this License.\n\n2.6. Fair Use\n\nThis License is not intended to limit any rights You have under\napplicable copyright doctrines of fair use, fair dealing, or other\nequivalents.\n\n2.7. Conditions\n\nSections 3.1, 3.2, 3.3, and 3.4 are conditions of the licenses granted\nin Section 2.1.\n\n3. Responsibilities\n-------------------\n\n3.1. Distribution of Source Form\n\nAll distribution of Covered Software in Source Code Form, including any\nModifications that You create or to which You contribute, must be under\nthe terms of this License. You must inform recipients that the Source\nCode Form of the Covered Software is governed by the terms of this\nLicense, and how they can obtain a copy of this License. You may not\nattempt to alter or restrict the recipients' rights in the Source Code\nForm.\n\n3.2. Distribution of Executable Form\n\nIf You distribute Covered Software in Executable Form then:\n\n(a) such Covered Software must also be made available in Source Code\n    Form, as described in Section 3.1, and You must inform recipients of\n    the Executable Form how they can obtain a copy of such Source Code\n    Form by reasonable means in a timely manner, at a charge no more\n    than the cost of distribution to the recipient; and\n\n(b) You may distribute such Executable Form under the terms of this\n    License, or sublicense it under different terms, provided that the\n    license for the Executable Form does not attempt to limit or alter\n    the recipients' rights in the Source Code Form under this License.\n\n3.3. Distribution of a Larger Work\n\nYou may create and distribute a Larger Work under terms of Your choice,\nprovided that You also comply with the requirements of this License for\nthe Covered Software. If the Larger Work is a combination of Covered\nSoftware with a work governed by one or more Secondary Licenses, and the\nCovered Software is not Incompatible With Secondary Licenses, this\nLicense permits You to additionally distribute such Covered Software\nunder the terms of such Secondary License(s), so that the recipient of\nthe Larger Work may, at their option, further distribute the Covered\nSoftware under the terms of either this License or such Secondary\nLicense(s).\n\n3.4. Notices\n\nYou may not remove or alter the substance of any license notices\n(including copyright notices, patent notices, disclaimers of warranty,\nor limitations of liability) contained within the Source Code Form of\nthe Covered Software, except that You may alter any license notices to\nthe extent required to remedy known factual inaccuracies.\n\n3.5. Application of Additional Terms\n\nYou may choose to offer, and to charge a fee for, warranty, support,\nindemnity or liability obligations to one or more recipients of Covered\nSoftware. However, You may do so only on Your own behalf, and not on\nbehalf of any Contributor. You must make it absolutely clear that any\nsuch warranty, support, indemnity, or liability obligation is offered by\nYou alone, and You hereby agree to indemnify every Contributor for any\nliability incurred by such Contributor as a result of warranty, support,\nindemnity or liability terms You offer. You may include additional\ndisclaimers of warranty and limitations of liability specific to any\njurisdiction.\n\n4. Inability to Comply Due to Statute or Regulation\n---------------------------------------------------\n\nIf it is impossible for You to comply with any of the terms of this\nLicense with respect to some or all of the Covered Software due to\nstatute, judicial order, or regulation then You must: (a) comply with\nthe terms of this License to the maximum extent possible; and (b)\ndescribe the limitations and the code they affect. Such description must\nbe placed in a text file included with all distributions of the Covered\nSoftware under this License. Except to the extent prohibited by statute\nor regulation, such description must be sufficiently detailed for a\nrecipient of ordinary skill to be able to understand it.\n\n5. Termination\n--------------\n\n5.1. The rights granted under this License will terminate automatically\nif You fail to comply with any of its terms. However, if You become\ncompliant, then the rights granted under this License from a particular\nContributor are reinstated (a) provisionally, unless and until such\nContributor explicitly and finally terminates Your grants, and (b) on an\nongoing basis, if such Contributor fails to notify You of the\nnon-compliance by some reasonable means prior to 60 days after You have\ncome back into compliance. Moreover, Your grants from a particular\nContributor are reinstated on an ongoing basis if such Contributor\nnotifies You of the non-compliance by some reasonable means, this is the\nfirst time You have received notice of non-compliance with this License\nfrom such Contributor, and You become compliant prior to 30 days after\nYour receipt of the notice.\n\n5.2. If You initiate litigation against any entity by asserting a patent\ninfringement claim (excluding declaratory judgment actions,\ncounter-claims, and cross-claims) alleging that a Contributor Version\ndirectly or indirectly infringes any patent, then the rights granted to\nYou by any and all Contributors for the Covered Software under Section\n2.1 of this License shall terminate.\n\n5.3. In the event of termination under Sections 5.1 or 5.2 above, all\nend user license agreements (excluding distributors and resellers) which\nhave been validly granted by You or Your distributors under this License\nprior to termination shall survive termination.\n\n************************************************************************\n*                                                                      *\n*  6. Disclaimer of Warranty                                           *\n*  -------------------------                                           *\n*                                                                      *\n*  Covered Software is provided under this License on an \"as is\"       *\n*  basis, without warranty of any kind, either expressed, implied, or  *\n*  statutory, including, without limitation, warranties that the       *\n*  Covered Software is free of defects, merchantable, fit for a        *\n*  particular purpose or non-infringing. The entire risk as to the     *\n*  quality and performance of the Covered Software is with You.        *\n*  Should any Covered Software prove defective in any respect, You     *\n*  (not any Contributor) assume the cost of any necessary servicing,   *\n*  repair, or correction. This disclaimer of warranty constitutes an   *\n*  essential part of this License. No use of any Covered Software is   *\n*  authorized under this License except under this disclaimer.         *\n*                                                                      *\n************************************************************************\n\n************************************************************************\n*                                                                      *\n*  7. Limitation of Liability                                          *\n*  --------------------------                                          *\n*                                                                      *\n*  Under no circumstances and under no legal theory, whether tort      *\n*  (including negligence), contract, or otherwise, shall any           *\n*  Contributor, or anyone who distributes Covered Software as          *\n*  permitted above, be liable to You for any direct, indirect,         *\n*  special, incidental, or consequential damages of any character      *\n*  including, without limitation, damages for lost profits, loss of    *\n*  goodwill, work stoppage, computer failure or malfunction, or any    *\n*  and all other commercial damages or losses, even if such party      *\n*  shall have been informed of the possibility of such damages. This   *\n*  limitation of liability shall not apply to liability for death or   *\n*  personal injury resulting from such party's negligence to the       *\n*  extent applicable law prohibits such limitation. Some               *\n*  jurisdictions do not allow the exclusion or limitation of           *\n*  incidental or consequential damages, so this exclusion and          *\n*  limitation may not apply to You.                                    *\n*                                                                      *\n************************************************************************\n\n8. Litigation\n-------------\n\nAny litigation relating to this License may be brought only in the\ncourts of a jurisdiction where the defendant maintains its principal\nplace of business and such litigation shall be governed by laws of that\njurisdiction, without reference to its conflict-of-law provisions.\nNothing in this Section shall prevent a party's ability to bring\ncross-claims or counter-claims.\n\n9. Miscellaneous\n----------------\n\nThis License represents the complete agreement concerning the subject\nmatter hereof. If any provision of this License is held to be\nunenforceable, such provision shall be reformed only to the extent\nnecessary to make it enforceable. Any law or regulation which provides\nthat the language of a contract shall be construed against the drafter\nshall not be used to construe this License against a Contributor.\n\n10. Versions of the License\n---------------------------\n\n10.1. New Versions\n\nMozilla Foundation is the license steward. Except as provided in Section\n10.3, no one other than the license steward has the right to modify or\npublish new versions of this License. Each version will be given a\ndistinguishing version number.\n\n10.2. Effect of New Versions\n\nYou may distribute the Covered Software under the terms of the version\nof the License under which You originally received the Covered Software,\nor under the terms of any subsequent version published by the license\nsteward.\n\n10.3. Modified Versions\n\nIf you create software not governed by this License, and you want to\ncreate a new license for such software, you may create and use a\nmodified version of this License if you rename the license and remove\nany references to the name of the license steward (except to note that\nsuch modified license differs from this License).\n\n10.4. Distributing Source Code Form that is Incompatible With Secondary\nLicenses\n\nIf You choose to distribute Source Code Form that is Incompatible With\nSecondary Licenses under the terms of this version of the License, the\nnotice described in Exhibit B of this License must be attached.\n\nExhibit A - Source Code Form License Notice\n-------------------------------------------\n\n  This Source Code Form is subject to the terms of the Mozilla Public\n  License, v. 2.0. If a copy of the MPL was not distributed with this\n  file, You can obtain one at https://mozilla.org/MPL/2.0/.\n\nIf it is not possible or desirable to put the notice in a particular\nfile, then You may include the notice in a location (such as a LICENSE\nfile in a relevant directory) where a recipient would be likely to look\nfor such a notice.\n\nYou may add additional accurate notices of copyright ownership.\n\nExhibit B - \"Incompatible With Secondary Licenses\" Notice\n---------------------------------------------------------\n\n  This Source Code Form is \"Incompatible With Secondary Licenses\", as\n  defined by the Mozilla Public License, v. 2.0.\n"}};
const MAX_TEXT = 2 * 1024 * 1024;
let standardSupplements = 0;
const replacements = [
  [root, '<project>'],
  ...(process.env.CARGO_HOME ? [[process.env.CARGO_HOME, '<cargo-cache>']] : []),
  [os.homedir(), '<user-home>'],
].sort((a, b) => b[0].length - a[0].length);
function clean(value) {
  let text = String(value);
  for (const [local, replacement] of replacements) {
    if (local.length > 1) text = text.replaceAll(local, replacement).replaceAll(local.replaceAll('\\', '/'), replacement);
  }
  return text;
}
function warn(pkg, message) { warnings.push(`${pkg}: ${message}`); }
function json(file) { return JSON.parse(fs.readFileSync(file, 'utf8')); }
function person(value) {
  if (typeof value === 'string') return value;
  if (value && typeof value === 'object') return [value.name, value.email ? `<${value.email}>` : '', value.url].filter(Boolean).join(' ');
  return '';
}
function spdx(value) {
  if (typeof value === 'string') return value;
  if (value && typeof value === 'object') return value.type || 'UNKNOWN';
  return 'UNKNOWN';
}
function within(parent, child) {
  const relative = path.relative(parent, child);
  return relative === '' || (!relative.startsWith(`..${path.sep}`) && relative !== '..' && !path.isAbsolute(relative));
}
const noticeName = /^(?:unlicen[cs]e|licen[cs](?:e|ing)|notice|copying|copyright|authors)(?:[._-].*)?$/i;
const licenseName = /^(?:unlicen[cs]e|licen[cs](?:e|ing)|copying)(?:[._-].*)?$/i;
const noticeDirectory = /^(?:licen[cs]es?|notices?)$/i;
function notices(directory, label, explicitFile, license) {
  const found = new Map();
  const sourceHeaders = new Set();
  let visited = 0;
  function add(file, isLicense) {
    if (found.has(file)) { if (isLicense) found.get(file).isLicense = true; return; }
    const relative = path.relative(directory, file).replaceAll(path.sep, '/');
    const stat = fs.lstatSync(file);
    if (!stat.isFile() || stat.isSymbolicLink()) return;
    if (stat.size > MAX_TEXT) { warn(label, `notice ${relative} exceeds 2 MiB and was not copied`); return; }
    const bytes = fs.readFileSync(file);
    if (bytes.includes(0)) { warn(label, `notice ${relative} is binary and was not copied`); return; }
    found.set(file, { relative, text: bytes.toString('utf8'), isLicense });
  }
  function visit(current, depth, inNotices = false) {
    if (depth > 5) return;
    for (const entry of fs.readdirSync(current, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      if (++visited > 20_000) return;
      if (entry.isSymbolicLink() || ['node_modules', '.git', 'target', '.cache'].includes(entry.name)) continue;
      const file = path.join(current, entry.name);
      if (entry.isDirectory()) visit(file, depth + 1, inNotices || noticeDirectory.test(entry.name));
      else if (entry.isFile() && (inNotices || noticeName.test(entry.name))) add(file, inNotices || licenseName.test(entry.name));
      else if (entry.isFile() && /\.(?:rs|c|h|cpp|hpp|js|mjs|ts)$/i.test(entry.name)) {
        // Preserve copyright/SPDX comments omitted from archive-level files.
        const fd = fs.openSync(file, 'r');
        let prefix;
        try { const bytes = Buffer.alloc(8192); prefix = bytes.subarray(0, fs.readSync(fd, bytes, 0, bytes.length, 0)).toString('utf8'); }
        finally { fs.closeSync(fd); }
        const header = prefix.match(/^\s*(?:(?:\/\/[^\n]*(?:\n|$))|(?:\/\*[\s\S]*?\*\/\s*))+/)?.[0]?.trim();
        if (header && /copyright|SPDX-License-Identifier|This Source Code Form/i.test(header)) sourceHeaders.add(header);
      }
    }
  }
  visit(directory, 0);
  if (visited > 20_000) warn(label, 'source-tree notice scan reached its file-count limit');
  if (explicitFile) {
    const file = path.resolve(directory, explicitFile);
    if (!within(directory, file)) warn(label, 'declared license_file is outside the packaged source and was not read');
    else if (fs.existsSync(file)) add(file, true);
    else warn(label, 'declared license_file is missing from the installed source');
  }
  if (sourceHeaders.size) found.set('<source headers>', { relative: 'Published source copyright / SPDX headers (deduplicated)', text: [...sourceHeaders].join('\n\n'), isLicense: false });
  if (![...found.values()].some(file => file.isLicense && file.text.trim())) {
    const normalized = (license || '').replaceAll('/', ' OR ');
    // Every affected locked dependency currently offers MIT, BSD-3-Clause or
    // MPL-2.0. When there is an OR choice, distribute under its MIT option.
    const choices = normalized.split(/\s+OR\s+/).map(choice => choice.trim().replace(/^\(|\)$/g, ''));
    const choice = choices.includes('MIT') ? 'MIT' : choices.find(choice => standardLicenses[choice]);
    if (choice && !/\bAND\b|\bWITH\b/.test(normalized)) {
      const standard = standardLicenses[choice];
      standardSupplements++;
      found.set('<standard terms>', {
        relative: `Canonical ${choice} license terms — SPDX License List v3.27.0`, isLicense: true,
        text: `The published archive omits a standalone license file. Its metadata declares ${license}.\nFor distribution under its ${choice} option, the complete standard text is supplied below.\nSource: ${standard.source}\nSPDX copyright placeholders are template fields, not assertions about upstream authors or years.\nActual published authors and available source copyright headers are reproduced separately above.\n\n${standard.text}`,
      });
    } else warn(label, `NO COMPLETE LICENSE TEXT FOUND (${license || 'UNKNOWN'}); inspect upstream before redistribution`);
  }
  return [...found.values()].sort((a, b) => a.relative.localeCompare(b.relative));
}

const installed = new Map();
const seenDirectories = new Set();
function packageDirectory(directory) {
  let real;
  try { real = fs.realpathSync(directory); } catch { return; }
  if (seenDirectories.has(real)) return;
  seenDirectories.add(real);
  const manifest = path.join(real, 'package.json');
  if (!fs.existsSync(manifest)) return;
  const data = json(manifest);
  if (data.name && data.version) {
    const id = `${data.name}@${data.version}`;
    if (!installed.has(id)) installed.set(id, { data, directory: real });
  }
  modulesDirectory(path.join(real, 'node_modules'));
}
function modulesDirectory(directory) {
  if (!fs.existsSync(directory)) return;
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    if (entry.name === '.pnpm') {
      for (const entry of fs.readdirSync(path.join(directory, '.pnpm'), { withFileTypes: true })) {
        if (entry.isDirectory()) modulesDirectory(path.join(directory, '.pnpm', entry.name, 'node_modules'));
      }
    } else if (entry.name.startsWith('.')) continue;
    else if (entry.name.startsWith('@')) {
      for (const child of fs.readdirSync(path.join(directory, entry.name))) packageDirectory(path.join(directory, entry.name, child));
    } else packageDirectory(path.join(directory, entry.name));
  }
}
if (!fs.existsSync(path.join(root, 'node_modules'))) throw new Error('Run pnpm install --frozen-lockfile before collecting dependency notices.');
modulesDirectory(path.join(root, 'node_modules'));
if (!installed.size) throw new Error('No installed JavaScript packages were found.');

const cargo = spawnSync(process.env.CARGO || 'cargo', ['metadata', '--locked', '--offline', '--format-version', '1', '--manifest-path', path.join(root, 'src-tauri/Cargo.toml')], {
  cwd: root, env: process.env, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, timeout: 120_000,
});
if (cargo.error || cargo.status !== 0) throw new Error('Cargo metadata could not be read. Install Rust, run cargo fetch --locked, and ensure cargo is on PATH (or set CARGO). No raw metadata or diagnostic paths were written.');
const metadata = JSON.parse(cargo.stdout);
const members = new Set(metadata.workspace_members);
const rust = metadata.packages.filter(pkg => !members.has(pkg.id)).sort((a, b) => `${a.name}@${a.version}`.localeCompare(`${b.name}@${b.version}`));

const sections = [
  'Streamer AI Cohost — dependency license and copyright notices',
  '============================================================',
  '',
  'Generated from installed JavaScript packages and Cargo.lock-resolved Rust package metadata.',
  'Includes development/build and target-specific dependencies when their source is installed.',
  'Original license/copyright/NOTICE/AUTHORS files are reproduced below; package metadata is informational.',
  'This collection does not relicense any dependency. Optional missing-text warnings appear at the end.',
  'Character-model and separately bundled font notices are supplied alongside this file.',
  '',
  `JavaScript packages: ${installed.size}; Rust packages: ${rust.length}`,
  '',
  'PROJECT LICENSE',
  '===============',
  fs.readFileSync(path.join(root, 'LICENSE'), 'utf8').trim(),
];
function section(kind, name, version, license, authors, files, source) {
  const label = `${kind}: ${name}@${version}`;
  sections.push('', '='.repeat(78), label, `License metadata: ${license || 'UNKNOWN'}`);
  const publishedAuthors = [...new Set(authors.filter(Boolean))];
  if (publishedAuthors.length) sections.push(`Authors / contributors from published metadata: ${publishedAuthors.join('; ')}`);
  if (source) sections.push(`Corresponding published package source: ${source}`);
  for (const file of files) sections.push('', `--- ${file.relative} ---`, file.text.trimEnd());
}
for (const [id, { data, directory }] of [...installed.entries()].sort(([a], [b]) => a.localeCompare(b))) {
  const authors = [person(data.author), ...(Array.isArray(data.contributors) ? data.contributors.map(person) : [])];
  const license = data.license ? spdx(data.license) : Array.isArray(data.licenses) ? data.licenses.map(spdx).join(' OR ') : 'UNKNOWN';
  section('JavaScript', data.name, data.version, license, authors, notices(directory, id, undefined, license), `https://www.npmjs.com/package/${data.name}/v/${data.version}`);
}
for (const pkg of rust) {
  const directory = path.dirname(pkg.manifest_path);
  section('Rust', pkg.name, pkg.version, pkg.license, pkg.authors || [], notices(directory, `${pkg.name}@${pkg.version}`, pkg.license_file, pkg.license), `https://crates.io/api/v1/crates/${pkg.name}/${pkg.version}/download`);
}
sections.push('', `Canonical standard-license supplements for archives missing standalone texts: ${standardSupplements}`, '', 'COLLECTION WARNINGS', '===================', ...(warnings.length ? warnings : ['None.']), '');
fs.mkdirSync(path.dirname(destination), { recursive: true });
fs.writeFileSync(destination, clean(sections.join('\n')));
console.log(`Wrote public/licenses/DEPENDENCIES.txt: ${installed.size} JavaScript and ${rust.length} Rust packages; ${standardSupplements} standard-license supplements; ${warnings.length} warning(s).`);
for (const warning of warnings) console.warn(`License notice warning: ${clean(warning)}`);
if (process.argv.includes('--strict') && warnings.length) process.exitCode = 1;
