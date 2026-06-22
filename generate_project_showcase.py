from reportlab.lib import colors
from reportlab.lib.pagesizes import LETTER
from reportlab.lib.styles import getSampleStyleSheet, ParagraphStyle
from reportlab.lib.units import inch
from reportlab.platypus import SimpleDocTemplate, Paragraph, Spacer, Table, TableStyle, ListFlowable, ListItem, KeepTogether
from reportlab.pdfbase.pdfmetrics import registerFontFamily
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.lib.enums import TA_CENTER, TA_LEFT

# Use built-in fonts for portability
styles = getSampleStyleSheet()
styles.add(ParagraphStyle(name='ProjTitle', parent=styles['Heading1'], fontSize=22, leading=26, alignment=TA_CENTER, textColor=colors.HexColor('#0F172A')))
styles.add(ParagraphStyle(name='ProjSubtitle', parent=styles['Heading2'], fontSize=12, leading=14, alignment=TA_CENTER, textColor=colors.HexColor('#475569')))
styles.add(ParagraphStyle(name='ProjSection', parent=styles['Heading2'], fontSize=13, leading=16, textColor=colors.HexColor('#0B5FFF')))
styles.add(ParagraphStyle(name='ProjBody', parent=styles['BodyText'], fontSize=9.5, leading=13, textColor=colors.HexColor('#1E293B')))
styles.add(ParagraphStyle(name='ProjSmall', parent=styles['BodyText'], fontSize=8.5, leading=11, textColor=colors.HexColor('#334155')))
styles.add(ParagraphStyle(name='ProjHighlight', parent=styles['BodyText'], fontSize=9.5, leading=13, textColor=colors.HexColor('#0F172A')))


def build_pdf(output_path):
    doc = SimpleDocTemplate(
        output_path,
        pagesize=LETTER,
        rightMargin=0.7 * inch,
        leftMargin=0.7 * inch,
        topMargin=0.6 * inch,
        bottomMargin=0.6 * inch,
    )

    story = []

    # Title block
    story.append(Paragraph('Toy Blockchain Project Showcase', styles['ProjTitle']))
    story.append(Paragraph('Rust-based blockchain prototype with consensus, networking, persistence, and API layers', styles['ProjSubtitle']))
    story.append(Spacer(1, 0.15 * inch))

    # Top summary box
    summary = Table(
        [[
            Paragraph('What this project demonstrates', styles['ProjSection']),
            Paragraph('Why it matters', styles['ProjSection'])
        ]],
        colWidths=[3.0 * inch, 3.0 * inch]
    )
    summary.setStyle(TableStyle([
        ('BACKGROUND', (0, 0), (-1, -1), colors.HexColor('#E8F0FF')),
        ('GRID', (0, 0), (-1, -1), 0.5, colors.HexColor('#C7D2FE')),
        ('VALIGN', (0, 0), (-1, -1), 'TOP'),
        ('PADDING', (0, 0), (-1, -1), 8),
    ]))
    story.append(summary)
    story.append(Spacer(1, 0.08 * inch))

    story.append(Paragraph('Core strengths', styles['ProjSection']))
    bullet_items = [
        'Systems-level Rust implementation with clear ownership and concurrency boundaries.',
        'End-to-end blockchain logic covering blocks, UTXO state, Merkle roots, and mining.',
        'Networked architecture for peer sync, chain replacement, and transaction broadcasting.',
        'Persistent storage and API design that make the project practical to inspect and run.',
    ]
    story.append(ListFlowable([
        ListItem(Paragraph(item, styles['ProjBody']), bulletColor=colors.HexColor('#2563EB'))
        for item in bullet_items
    ], bulletType='bullet', bulletFontName='Helvetica', bulletFontSize=9.5, leftIndent=18))
    story.append(Spacer(1, 0.15 * inch))

    # Two-column technical highlights
    tech_left = [
        Paragraph('Architecture highlights', styles['ProjSection']),
        Paragraph('• Consensus & block validation', styles['ProjBody']),
        Paragraph('• UTXO transaction model', styles['ProjBody']),
        Paragraph('• Merkle tree verification', styles['ProjBody']),
        Paragraph('• P2P synchronization logic', styles['ProjBody']),
        Paragraph('• REST API for node inspection', styles['ProjBody']),
    ]
    tech_right = [
        Paragraph('Notable implementation areas', styles['ProjSection']),
        Paragraph('• Rust modules for blockchain, network, mempool, and transactions', styles['ProjBody']),
        Paragraph('• Persistent storage using sled', styles['ProjBody']),
        Paragraph('• Cryptographic verification with ed25519 and SHA-256', styles['ProjBody']),
        Paragraph('• Test coverage for validation and chain behavior', styles['ProjBody']),
        Paragraph('• Seed data and example state for reproducible demos', styles['ProjBody']),
    ]

    table = Table(
        [[tech_left, tech_right]],
        colWidths=[3.2 * inch, 3.2 * inch]
    )
    table.setStyle(TableStyle([
        ('VALIGN', (0, 0), (-1, -1), 'TOP'),
        ('LEFTPADDING', (0, 0), (-1, -1), 0),
        ('RIGHTPADDING', (0, 0), (-1, -1), 0),
    ]))
    story.append(table)
    story.append(Spacer(1, 0.15 * inch))

    # Bottom section with technologies and takeaways
    story.append(Paragraph('Technology stack', styles['ProjSection']))
    tech_tags = [
        'Rust', 'Tokio', 'Axum', 'Serde', 'sled', 'SHA-256', 'ed25519', 'Merkle Trees', 'REST API', 'P2P'
    ]
    tag_text = '  '.join([f'<font color="#2563EB">■</font> {tag}' for tag in tech_tags])
    story.append(Paragraph(tag_text, styles['ProjSmall']))
    story.append(Spacer(1, 0.1 * inch))

    story.append(Paragraph('Why this should stand out', styles['ProjSection']))
    story.append(Paragraph(
        'This project combines core software engineering skills—system design, data modeling, networking, cryptography, testing, and API development—into a single, runnable demonstration. It is especially strong for showing practical reasoning about reliability, correctness, and distributed behavior.',
        styles['ProjBody']
    ))

    doc.build(story)


if __name__ == '__main__':
    build_pdf('project_showcase.pdf')
    print('Generated project_showcase.pdf')
