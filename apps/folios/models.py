from django.db import models
from apps.core.models import HotelScopedModel
class Folio(HotelScopedModel):
    reservation=models.OneToOneField('reservations.Reservation',on_delete=models.PROTECT)
    total_charges=models.DecimalField(max_digits=12,decimal_places=2,default=0)
    total_payments=models.DecimalField(max_digits=12,decimal_places=2,default=0)
    balance=models.DecimalField(max_digits=12,decimal_places=2,default=0)
class FolioItem(HotelScopedModel):
    type=models.CharField(max_length=30); amount=models.DecimalField(max_digits=12,decimal_places=2); memo=models.CharField(max_length=200,blank=True)
    folio=models.ForeignKey(Folio,on_delete=models.PROTECT,related_name='items'); is_void=models.BooleanField(default=False)
