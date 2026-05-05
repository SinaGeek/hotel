from django.db import models
from apps.core.models import HotelScopedModel

class PaymentMethod(models.TextChoices):
    CASH = 'CASH'
    TRANSFER = 'TRANSFER'
    CARD = 'CARD'

class Payment(HotelScopedModel):
    method=models.CharField(max_length=20, choices=PaymentMethod.choices); status=models.CharField(max_length=20, default='PENDING')
    amount=models.DecimalField(max_digits=12,decimal_places=2); folio=models.ForeignKey('folios.Folio',on_delete=models.PROTECT)
    terminal_id=models.CharField(max_length=40, blank=True); rrn=models.CharField(max_length=64, blank=True)
    refunded_payment=models.ForeignKey('self',null=True,blank=True,on_delete=models.PROTECT)

class CashShift(HotelScopedModel):
    opened_by=models.ForeignKey('accounts.User',related_name='opened_shifts',on_delete=models.PROTECT)
    closed_by=models.ForeignKey('accounts.User',related_name='closed_shifts',null=True,blank=True,on_delete=models.PROTECT)
    opening_balance=models.DecimalField(max_digits=12,decimal_places=2); closing_balance=models.DecimalField(max_digits=12,decimal_places=2,null=True,blank=True)
    expected_balance=models.DecimalField(max_digits=12,decimal_places=2,default=0); actual_balance=models.DecimalField(max_digits=12,decimal_places=2,null=True,blank=True)
